//! Senda's retained scenery: the ground the neighbourhood stands on (the lots' small
//! lawns with planted beds in their front yards, the greenbelt's dense mottled scrub,
//! the groundcover along Senda Parkway, the mulched and planted slopes of the bank below it
//! and round the tract, and the grey-green sage of the hillside, rising past the arena
//! into green rolling hills), Senda Parkway with its kerbed, planted median, bike lanes and
//! the wide path along its south side, Neblina, Rumbo and Gaviota paved along their
//! traced curves with kerbs, parkways and sidewalks, the cul-de-sacs and their islands,
//! each house's driveway, front walk and entry bed behind a stucco seat wall, the walled
//! back yards (decomposed granite, a patio, a lawn, planted beds with low shrubs and a
//! small tree, on most lots
//! a tile-roofed rear loggia or a pergola, a spa, a fire pit or a potted tree), the
//! greenbelt's thicket sloping up to Rumbo with its creek half hidden in it and its walk,
//! the planted slopes behind Neblina's west row (rosemary, agaves and olives under the
//! dark perimeter block) and Gaviota's south row (scrub under a band of trees), the
//! terrace walk and spur walks on the bank and the drains round the cul-de-sacs' ends, the
//! spawn pads,
//! and oaks and sage past the arena's edge. The lots' walls are cover
//! ([`crate::sim::senda_yards`]); [`yard_wall`] draws them.
//!
//! The paving is drawn, not collided: tanks drive on the arena's flat floor, and the
//! slabs stand a few centimetres proud of the grass as the village roads float.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::{DQuat, DVec2};

use crate::geometry::{
    Mesh, RingGeometry, box_geometry, cylinder_geometry, icosahedron_geometry, octahedron_geometry,
    plane_geometry_segments, sphere_geometry,
};
use crate::scene::Node;
use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
use crate::sim::house_layout::{HouseDesign, SENDA_STYLES, wall_frames};
use crate::sim::kinds::{CoverKind, Team};
use crate::sim::model_data::ARENA;
use crate::sim::senda_layout::{
    self, CUL_DE_SAC_ISLAND, CUL_DE_SAC_RADIUS, HOUSE_DEPTH, HOUSE_WIDTH, Line, MEDIAN_HALF,
    MEDIAN_TREE_SPACING, PARKWAY, Row, SENDA_PICKUPS, SIDE_YARD, SIDEWALK, SOUTH_LANE,
    SPLITTER_ISLAND, SendaHouse, Street, WAY_PARKWAY, WAY_PATH, WAY_WALL_ROOM, clear_of_wall,
    greenbelt_mouth, house_front, in_arena, in_median_opening, inside_boundary, is_senda_paved,
    left, neblina_bend, oak_site, outer_way_tree_sites, senda_boundary, senda_houses,
    way_tree_sites,
};
use crate::sim::senda_yards::{
    LOGGIA_COLUMN, LOGGIA_EAVE, Lot, PATIO_POST, PATIO_ROOF, PERGOLA_WOOD, PatioCover,
    YARD_GATE_COLOR, YARD_PERIMETER_COLOR, YARD_PLANTER, YARD_VIEW_COLOR, YARD_WALL_COLOR,
    YARD_WALL_THICKNESS, greenbelt_lane, house_back, park_corners, patio_depth, senda_lots,
    turf_side,
};
use crate::sim::tree_proportions::tree_proportions;

use super::batching::{batch, batch_in_cells, paint_mesh};
use super::building_kit::{textured, tint_over};
use super::concrete_surfaces::CONCRETE_TEXTURE;
use super::ground_surfaces::{GroundKind, ground_material, ground_uvs};
use super::model_primitives::{
    box_part, cylinder_part, paint, put, rotated, shadow_receiver, shadowed,
};
use super::scenery::create_spawn_pads;
use super::tree_models::{TreeDetail, TreeShape, leafy_shrub, tree_model};

/// From a residential street's centre line to the back of its sidewalk (m).
const STREET_HALF_WALK: f64 = crate::sim::senda_layout::STREET_HALF + PARKWAY + SIDEWALK;
/// How far the grass reaches either way from the arena's centre.
const GROUND_EXTENT: f64 = 300.0;
/// Paving colours: weathered asphalt, sidewalk and kerb concrete, driveways a warmer
/// broom-finished concrete, and Senda Parkway's planted median, mulch and shrubs under
/// its trees.
const ASPHALT: u32 = 0x4e5052;
const SIDEWALK_CONCRETE: u32 = 0xc4bfb3;
const KERB: u32 = 0xd3cec2;
const DRIVEWAY: u32 = 0xbdb3a1;
const MEDIAN: u32 = 0x7e8a86;
/// Slab thickness and kerb height and width (m).
const SLAB: f64 = 0.05;
const KERB_HEIGHT: f64 = 0.13;
const KERB_WIDTH: f64 = 0.16;
/// Paving heights over the floor: the streets, then the kerbs, sidewalks and median
/// (raised over the asphalt they cross so junctions do not flicker).
const STREET_TOP: f64 = 0.045;
const WALK_TOP: f64 = 0.06;
/// How much wider a driveway is than its garage door, the front walk's width, and the
/// strip of brown pavers bordering the driveway on the front walk's side (width, paint).
const DRIVEWAY_PAST_DOOR: f64 = 0.15;
const FRONT_WALK: f64 = 0.9;
const PAVER_STRIP: (f64, u32) = (0.25, 0x8e7764);
/// Ground tints over the dry-grass tile, linear: they multiply its mustard straw, and a
/// blue past 1 draws it back toward grey-green. The small lawns front and back a
/// watered but dusty green, the greenbelt's irrigated grass and groundcover a mid green
/// a little yellower than the lawns, the low groundcover along Senda Parkway a grey green,
/// the planted slopes (the bank under Senda Parkway, the slope across it and round the
/// tract) an olive green in drifts over a little brown mulch, and the coastal sage scrub
/// of the hillside east of the cul-de-sacs a duller olive with a few dry patches and
/// grey-green shrub islands.
const LAWN: [f32; 3] = [0.36, 0.5, 0.85];
/// The greenbelt's scrub, mottled over a few metres between olive, grey-green and dark
/// green.
const GREENBELT_TONES: [[f32; 3]; 3] = [[0.26, 0.38, 0.5], [0.3, 0.45, 0.8], [0.17, 0.26, 0.38]];
const GROUNDCOVER: [f32; 3] = [0.36, 0.44, 0.95];
const MULCH: [f32; 3] = [0.46, 0.44, 0.9];
/// The front yards' beds along the houses: dark bark mulch.
const BED_TINT: [f32; 3] = [0.36, 0.33, 0.8];
/// The slope south of Gaviota's south row down toward Gardenia, and the bank's lower
/// third: dark groundcover over mulch under the trees.
const SHADED_COVER: [f32; 3] = [0.3, 0.38, 0.9];
const BANK_FOOT: [f32; 3] = [0.28, 0.36, 0.9];
const SAGE: [f32; 3] = [0.35, 0.5, 1.5];
/// The bank under Senda Parkway: grey-green groundcover drifting over bark mulch, and how
/// much of it is groundcover at least and at most.
const BANK_COVER: [f32; 3] = [0.38, 0.43, 1.0];
const BANK_PLANTED: (f64, f64) = (0.25, 0.55);
/// How much of the other planted slopes (round the tract) is groundcover, at least and
/// at most.
const SLOPE_PLANTED: (f64, f64) = (0.35, 0.7);
/// The planted ground's tints over the packed-dirt tile, linear (it is an orange tan, so
/// they draw its red down and its blue up): bark mulch, the groundcover drifting through
/// it, the shaded groundcover under the slopes' trees and the front beds' darker mulch;
/// it reaches this far (m) past the arena's edge, in cells this fine (m).
const DIRT_MULCH: [f32; 3] = [0.3, 0.34, 0.85];
const DIRT_COVER: [f32; 3] = [0.24, 0.4, 0.85];
const DIRT_SHADE: [f32; 3] = [0.15, 0.27, 0.75];
const DIRT_BED: [f32; 3] = [0.22, 0.25, 0.7];
/// The groundcover on the greenbelt's mouth, blue-grey senecio.
const MOUTH_COVER: [f32; 3] = [0.3, 0.85, 2.4];
/// The bank's bark mulch, grey-brown rather than the slopes' warmer brown.
const DIRT_BANK_MULCH: [f32; 3] = [0.4, 0.68, 2.1];
/// How much of the ground behind the outer rows is groundcover rather than shade, at
/// least and how much more it drifts.
const UNDER_TREES_COVER: (f64, f64) = (0.25, 0.35);
const PLANTED_REACH: f64 = 12.0;
const PLANTED_STEP: f64 = 1.0;
/// The hills past the arena, green with the winter rains, and the dry tan patches between
/// their green.
const HILLS: [f32; 3] = [0.4, 0.62, 0.78];
const HILLS_DRY: [f32; 3] = [0.7, 0.66, 0.82];
/// How far (m) past the houses' fronts the front yards' planted beds reach (the ground's
/// vertices are a few metres apart, so its edge blurs over the side yards), and how much
/// of a bed is groundcover rather than mulch.
const FRONT_BEDS_REACH: f64 = 1.0;
const FRONT_BEDS_COVER: f64 = 0.45;
/// The hillside's natural scrub begins this far east (arena x) and blends in over this
/// many metres; west of it the slopes are planted.
const HILLSIDE_FROM: f64 = 48.0;
/// How much of the hillside inside the arena is dry tan grass and sage scrub at least,
/// between the drifts that add more.
const HILLSIDE_DRY: f64 = 0.72;
/// The hillside's dry grass and sage inside the arena, tan olive over the dry-grass tile
/// (linear), the winter green only in its swales.
const HILLSIDE_TAN: [f32; 3] = [1.05, 1.25, 3.0];
const HILLSIDE_SCRUB: f64 = 0.15;
const HILLSIDE_BLEND: f64 = 5.0;
/// How much darker the scrub grows at the foot of the bank below Senda Parkway than at
/// its top, and over how many metres down from the path it darkens.
const BANK_SHADE: f32 = 0.3;
const BANK_FALL: f64 = 10.0;
/// The terrace drains along the slopes' contours: narrow concrete V-ditches, their width,
/// paint and the darker crease down their middle (its width).
const TRAIL_WIDTH: f64 = 0.6;
/// The hillside's wider terrace drains (m).
const HILL_TRAIL_WIDTH: f64 = 1.0;
const TRAIL: u32 = 0xc4c1b8;
const TRAIL_CREASE: (f64, u32) = (0.1, 0x9c998f);
/// The terrace walk along the bank under Senda Parkway: this share of the way down from the
/// Way's path to the back walls, where the bank is at least this wide (m), this wide (m),
/// with a paler edge this wide (m) either side; and the straight spur walks from it up to
/// the Way's path, every three or four lots, at these arena x, this wide (m).
const BANK_TERRACE_DOWN: f64 = 0.33;
const BANK_TRAIL_ROOM: f64 = 4.0;
const BANK_WALK: f64 = 1.2;
const BANK_WALK_EDGE: (f64, u32) = (0.08, 0xd8d3c6);
const BANK_SPURS: [f64; 3] = [-14.0, 8.0, 27.0];
const BANK_SPUR: f64 = 0.9;
/// The trails on the hillside east of the cul-de-sacs: each keeps this far (m) out from
/// the neighbourhood's east edge (the back fences round the cul-de-sacs and the
/// turning circles), so it curves round the ends of Rumbo and Gaviota as the real
/// ones do, smoothed over this many metres, from and to these arena z; and how far a
/// trail keeps from a spawn pad's middle.
const HILLSIDE_TRAIL_REACH: [f64; 3] = [6.0, 14.0, 25.0];
const HILLSIDE_TRAIL_SMOOTH: f64 = 6.0;
const HILLSIDE_TRAIL_SPAN: (f64, f64) = (-70.0, 72.0);
const TRAIL_SPAWN_ROOM: f64 = 4.5;
/// How far (m) a hillside trail's middle keeps from the face of the wall along the
/// arena's east edge.
const TRAIL_WALL_ROOM: f64 = 1.0 + TRAIL_WIDTH / 2.0;
/// The shrubs that grow in drifts along the hillside trails: how far (m) from a trail's
/// middle.
const TRAIL_SHRUBS: (f64, f64) = (1.0, 2.4);
/// The lots' yards inside their walls: decomposed granite over the side and back
/// yards, a patio against the house's back wall (concrete and pavers by turns) about
/// this share of the yard deep, but no deeper than the tract's patios (m) nor shallower
/// than the least (m), on every other lot a lawn behind it over this share of the lot's
/// width (a dark watered green over the grass tile, linear), a planted bed of bark mulch
/// along the back wall (its width, m) and narrower ones along the side walls behind the
/// patio.
const YARD_GRAVEL: u32 = 0x9d907a;
const PATIO_CONCRETE: u32 = 0xd2cbbd;
const PATIO_PAVERS: u32 = 0xb59f86;
const TURF: [f32; 3] = [0.42, 1.12, 1.25];
const TURF_SHARE: f64 = 0.55;
const SIDE_PLANTER: f64 = 0.6;
/// The back yards' planting: low shrubs along the beds, about one every this many metres
/// (their size range, m, and leaves, dark glossy greens with a flowering one), and a small
/// tree in a back corner of each yard (its crown's breadth and height, m): a citrus's dark
/// round crown or a crape myrtle's lighter, taller one, by turns.
const YARD_SHRUB_STEP: f64 = 1.1;
const YARD_SHRUB: (f64, f64) = (0.3, 0.48);
const YARD_SHRUB_LEAVES: [u32; 4] = [0x3f5a35, 0x4a6238, 0x55603f, 0x7a4a52];
const YARD_TREE: [(f64, f64, u32); 3] = [
    (1.7, 2.9, 0x3d5632),
    (1.9, 3.4, 0x5c6c44),
    (2.1, 3.2, 0x5f6750),
];
/// The dark planting along the side walls behind the patios: a shrub every this many
/// metres, this size range (radius, m), in dark glossy greens.
const SIDE_SHRUB_STEP: f64 = 1.0;
const SIDE_SHRUB: (f64, f64) = (0.3, 0.42);
const SIDE_SHRUB_LEAVES: [u32; 3] = [0x3a5233, 0x44593a, 0x4f5f3c];
/// The bark mulch along the outside of every back wall, at the foot of the bank and the
/// slopes (its width, m).
const WALL_FOOT: f64 = 1.0;
/// The rear loggias' tile courses: their spacing down the slope and the butts' depth (m).
const LOGGIA_TILE_COURSE: (f64, f64) = (0.32, 0.03);
/// How much darker than the tile its courses' shadowed butts are.
const LOGGIA_BUTT_SHADE: f64 = 0.72;
/// A fire pit on every third lot with no spa: its pale stone ring's radius and height and
/// the dark ash bowl inside (m).
const FIRE_PIT: (f64, f64) = (0.45, 0.3);
const FIRE_PIT_STONE: u32 = 0xb5ab98;
const FIRE_PIT_ASH: u32 = 0x3b3835;
const PERGOLA_SLAT: (f64, f64) = (0.07, 0.32);
/// A round spa on every [`SPA_EVERY`]th lot: its radius and height (m), its coping and
/// its water.
const SPA_EVERY: usize = 6;
const SPA: (f64, f64) = (0.7, 0.45);
const SPA_COPING: u32 = 0xc9bfac;
const SPA_WATER: u32 = 0x4f7d86;
/// A trampoline on the turf of a few lots: its radius and its mat's height (m), its
/// black mat and legs and its blue safety pad.
const TRAMPOLINE: (f64, f64) = (0.9, 0.75);
const TRAMPOLINE_EVERY: usize = 12;
const TRAMPOLINE_MAT: u32 = 0x1d1f22;
const TRAMPOLINE_PAD: u32 = 0x2f5d7a;
/// A patio umbrella on some lots: its canopy's radius and its pole's height (m), and
/// the canvas, cream or terracotta.
const UMBRELLA: (f64, f64) = (1.0, 2.2);
const UMBRELLA_CANVAS: (u32, u32) = (0xd9cfb2, 0x9c4a36);
/// The potted tree in a back corner of the lots with no spa: the terracotta pot's radius
/// and height and the crown's (m).
const POT: (f64, f64, u32) = (0.24, 0.42, 0xa65e3e);
const POT_CROWN: (f64, f64) = (0.45, 1.3);
/// The front yards: the parkways' clipped lawn, brighter than the yards' (sRGB paint, a
/// flat slab so its edges cut clean at the aprons), the front lawn panels, and the
/// depth (m) of the dark planting bed along each house's front that they stop short of.
/// The front lawns' and parkways' tint over the dry-grass tile, linear: a muted mid green.
const FRONT_LAWN_TINT: [f32; 3] = [0.5, 0.86, 1.75];
/// The bend park's kidney lawn (sRGB paint).
const PARK_GRASS: u32 = 0x5c6a40;
const FRONT_BED: f64 = 1.0;
/// The front beds' ornamental grass: a tuft every this many metres along the bed, and how
/// far (m) the bed's planting keeps from the middle of the garage's driveway.
const FRONT_BED_GRASS: f64 = 0.5;
/// Clipped boxwood along the seat walls and the houses' fronts: a ball every this many
/// metres, this size range (m), in dark green.
const BOXWOOD_STEP: f64 = 0.45;
const BOXWOOD: (f64, f64) = (0.2, 0.24);
const BOXWOOD_LEAVES: [u32; 2] = [0x3f5a35, 0x3a5232];
/// The brick cap on the seat walls of the houses with a brick porch.
const BRICK_CAP: u32 = 0x8a4c3a;
const GARAGE_BED_ROOM: f64 = 1.6;
/// The little park on the outside of Neblina's bend: its kidney lawn's half width and
/// half depth as shares of the park's, and the bite out of its street side; the walk's
/// width (m); the mulch of its beds; its benches' length and seat height (m) and wood.
const PARK_LAWN: (f64, f64, f64) = (0.5, 0.48, 0.28);
const PARK_WALK: f64 = 1.2;
const PARK_MULCH: u32 = 0x6a5a45;
const BENCH: (f64, f64) = (1.3, 0.45);
const BENCH_WOOD: u32 = 0x5a4632;
/// The planted bed outside a row's end lot's view fence toward the street or the
/// hillside, as the corner lots' are: tall ornamental grass (lomandra) over a blue-grey
/// groundcover (senecio). The bed's width and how far (m) its pieces run, the
/// groundcover's paint, and the grass: a tuft every this far (m) along each of its two
/// staggered lines, each tuft's blades' width, its height range (m), its blades and their
/// leaves.
const SIDE_BED: (f64, f64) = (1.3, 1.0);
const SIDE_BED_COVER: u32 = 0x76847f;
const SIDE_BED_TUFT: f64 = 0.55;
const TALL_GRASS: (f64, f64, f64) = (0.06, 0.8, 1.0);
const TALL_GRASS_BLADES: usize = 9;
/// How tall a tuft of the lower grasses stands for its size and squash.
const TUFT_HEIGHT: f64 = 1.5;
const TALL_GRASS_LEAVES: [u32; 3] = [0x5a6236, 0x6a6a42, 0x60673a];
/// The red aloes among the agaves on the corner wedge where Rumbo leaves Neblina: how far
/// along Rumbo past Neblina's sidewalk and how far out past its own (m), their size (m),
/// their grey-green leaves and their orange-red flower spikes (height, m, and paint).
const CORNER_ALOES: [(f64, f64); 3] = [(0.4, 1.5), (1.7, 2.4), (2.4, 1.3)];
const ALOE: (f64, u32) = (0.3, 0x6f7d58);
const ALOE_SPIKE: (f64, u32) = (0.7, 0xd0582c);
/// The band of trees down the slope behind Gaviota's south row, past the level's wall:
/// one every this many metres in staggered rows, out to this far (m) from the wall,
/// their crowns' breadth and their heights at least and at most (m).
const CANOPY_SPACING: f64 = 5.2;
const CANOPY_BAND: f64 = 9.0;
const CANOPY_CROWN: (f64, f64) = (3.8, 4.8);
const CANOPY_HEIGHT: (f64, f64) = (4.5, 6.2);
/// How far (m) a slope tree's trunk keeps from the outer face of the level's wall.
const CANOPY_WALL_ROOM: f64 = 1.0;
/// The greenbelt's path: a straight concrete walk along the foot of Gaviota's north row's
/// back walls, from its mouth on Neblina to the hillside: its width and how far (m) its
/// near edge keeps from the walls.
const PATH_WIDTH: f64 = 0.9;
const PATH_OFFSET: f64 = 1.0;
/// Senda Parkway's lane lines: thin white paint, one solid line each way along its bike
/// lane, this far in from the kerb; at Neblina the Way has one travel lane each way.
const LANE_PAINT: u32 = 0xe4e2da;
const LANE_LINE: f64 = 0.12;
const BIKE_LANE: f64 = 0.9;
/// Neblina's double yellow centre line, which marks it as the tract's collector from
/// Senda Parkway down to where it becomes Gaviota: its paint, each line's width and the gap
/// between the two (m).
const CENTRE_LINE_PAINT: u32 = 0xd2ad3c;
const CENTRE_LINE: f64 = 0.1;
const CENTRE_LINE_GAP: f64 = 0.15;
/// Acorn street lamps: a dark pole with a pale acorn globe under a cap, standing in the
/// parkway by the side yard of every [`LAMP_EVERY`]th house (m, paints).
const LAMP_EVERY: usize = 5;
const LAMP_HEIGHT: f64 = 2.8;
const LAMP_POLE: f64 = 0.055;
const LAMP_GLOBE: f64 = 0.16;
const LAMP_METAL: u32 = 0x2a3a2e;
const LAMP_GLASS: u32 = 0xf2eee2;
/// Senda Parkway's tall lamps along its south parkway: one every this many metres, their
/// height and globe (m).
const WAY_LAMP_SPACING: f64 = 2.0 * MEDIAN_TREE_SPACING;
const WAY_LAMP_HEIGHT: f64 = 4.6;
const WAY_LAMP_GLOBE: f64 = 0.22;
/// Trees outside the walls: one every this many metres on the slopes either side of
/// Senda Parkway, and the hillside's few oaks among its sage.
const SLOPE_TREE_SPACING: f64 = 11.0;
const HILLSIDE_OAK_SPACING: f64 = 26.0;
/// Shrub clumps of two or three lumps: a clump every so many metres on a jittered grid
/// (the hillside's sage, the planted slopes' shrubs, the greenbelt's scrub, the
/// ornamental grasses down Senda Parkway's median and on the corner beds), their leaves
/// and their lumps' size range (m).
const SAGE_SPACING: f64 = 5.5;
const SAGE_LEAVES: [u32; 3] = [0x5a5e40, 0x63653f, 0x52583a];
const SAGE_LUMP: (f64, f64) = (0.55, 1.0);
const PLANTED_SPACING: f64 = 2.6;
/// The planted slopes' shrubs grow in drifts where a slow noise from -2 to 2 passes
/// this level.
const PLANTED_DRIFT: f64 = 0.3;
const PLANTED_LEAVES: [u32; 3] = [0x5c6046, 0x666a4e, 0x52583e];
const PLANTED_LUMP: (f64, f64) = (0.4, 0.75);
/// The greenbelt's riparian thicket: a continuous dense mat of willow, mulefat and
/// toyon from back wall to back wall but for its walk and its creek, mounded, drawn as
/// one surface over a grid this fine (m); its mounds rise this high at least and at most
/// (m), rippled by this much (m) of fine texture, in dark winter greens with darker shade
/// in its hollows (tints over the dry-grass tile, linear, as the ground's); it rises over
/// this many metres from this far off the walls, the walk, a pickup and a spawn pad.
const MAT_STEP: f64 = 0.5;
const MAT_HEIGHT: (f64, f64) = (0.25, 0.6);
/// The thicket's shrubs' spacing (m): each a dome about this wide.
const MAT_DOME: f64 = 1.4;
const MAT_RIPPLE: f64 = 0.16;
const MAT_TONES: [[f32; 3]; 3] = [[0.12, 0.26, 0.4], [0.15, 0.31, 0.5], [0.09, 0.2, 0.3]];
const MAT_SHADE: [f32; 3] = [0.05, 0.13, 0.15];
/// The greenbelt slopes up from Gaviota's north row to Rumbo's south row: its thicket
/// stands this share of its height at the Gaviota side's walk and this much taller (m)
/// against Rumbo's back fences.
const MAT_SLOPE: (f64, f64) = (0.75, 0.3);
const MAT_RISE: f64 = 0.7;
const MAT_WALL_ROOM: f64 = 0.3;
const MAT_PATH_ROOM: f64 = 0.5;
/// The shrubs standing out of the mat, so the thicket reads as a lumpy mass of many
/// bushes: one every this many metres or so, their radius range (m) and leaves, dark
/// toyon and willow green, mulefat's yellow green and coyote brush's grey green.
const MAT_BUSH_SPACING: f64 = 1.25;
const MAT_BUSH: (f64, f64) = (0.55, 1.0);
const MAT_BUSH_LEAVES: [u32; 5] = [0x46523a, 0x8a9150, 0x6f7a5c, 0x3e4a34, 0x5b6a3e];
/// Small willows and sycamores along the creek, standing out of the thicket (scenery a
/// tank pushes through): one every this many metres along it, their crowns' breadth and
/// their heights at least and at most (m), kept this far (m) from a spawn pad, a pickup
/// and the greenbelt's cover trees.
const CREEK_TREE_STEP: f64 = 8.0;
const CREEK_TREE_CROWN: (f64, f64) = (2.4, 3.2);
const CREEK_TREE_HEIGHT: (f64, f64) = (3.6, 5.0);
const CREEK_TREE_ROOM: f64 = 3.5;
/// The broadleaf family [`tree_proportions`] grows as a pale-barked, open-crowned tree,
/// standing in for the creek's sycamores and willows.
const PALE_BROADLEAF: u32 = 4;
/// The creek down the greenbelt's middle, a narrow runnel of dull grey-brown silt and
/// stones sunk in the thicket: its width (m) and colour; its points lie this far apart
/// (m), and it meanders this far (m) either side of the lane's middle, on two waves
/// (radians per metre). The thicket closes over it but for stretches where a slow wave
/// along it passes [`CREEK_SHOWS`].
const CREEK: f64 = 0.45;
const CREEK_BED: u32 = 0x7a705f;
const CREEK_STEP: f64 = 0.45;
const CREEK_MEANDER: (f64, f64, f64) = (1.4, 0.21, 0.53);
const CREEK_SHOWS: f64 = 0.1;
/// The tubular steel view fence across the greenbelt's mouth on Neblina, on block
/// pilasters: its height, the pilasters' side and spacing, the pickets' side and spacing
/// (m), and its black paint.
const VIEW_FENCE: (f64, f64, f64) = (1.1, 0.4, 2.6);
const PICKETS: (f64, f64) = (0.025, 0.12);
const FENCE_STEEL: u32 = YARD_VIEW_COLOR;
/// The planted slopes' shrubs, round and neat on their mulch as the tract's are: set out
/// in contour rows this far apart (m) down the slope, about one every so many metres
/// along a row (every other row half a step on), each nudged off its row by up to this
/// share of the spacing, this share of the sites left bare; shrubs about a metre across
/// (radius range, m) in three dark grey-olives.
const CONTOUR_ROW: f64 = 2.2;
const CONTOUR_STEP: f64 = 1.7;
const CONTOUR_JITTER: f64 = 0.25;
const CONTOUR_BARE: f64 = 0.18;
const CONTOUR_SIZE: (f64, f64) = (0.4, 0.55);
const CONTOUR_LEAVES: [u32; 3] = [0x58603f, 0x4e5638, 0x5f6845];
/// The planted slope behind Neblina's west row, Senda Parkway's west branch's parkway as it
/// climbs to the tract's rear wall: one dense mat of mounded rosemary and salvia, grey-green
/// and silvery, its mounds this high at least and at most (m), standing this much taller
/// (m) toward the back walls, where the real slope rises; the mat fills a grid this fine
/// (m) and stays this far (m) off a spawn pad's middle (its rim and a tank's way off it)
/// and a pickup.
const WEST_MAT: (f64, f64) = (0.3, 0.75);
const WEST_MAT_RISE: f64 = 0.3;
const OUTER_MAT_STEP: f64 = 0.5;
/// The slopes' shrubs' spacing (m): rosemary and salvia mounds on the west slope, broader
/// scrub on the south one.
const WEST_DOME: f64 = 1.1;
const SOUTH_DOME: f64 = 1.6;
const OUTER_PAD_ROOM: f64 = 2.3 + 2.5;
const OUTER_PICKUP_ROOM: f64 = 2.0;
const WEST_MAT_TONES: [[f32; 3]; 3] = [[0.15, 0.35, 0.56], [0.18, 0.4, 0.66], [0.12, 0.28, 0.45]];
const WEST_MAT_SHADE: [f32; 3] = [0.05, 0.09, 0.11];
/// Behind Neblina's west row the mat stands full and tall in a band this deep (m) from
/// the walls' mulch, under the trees; past it, out to the spawn lane, the field is bark
/// mulch with low groundcover in drifts, this high at least and at most (m), where a slow
/// noise passes [`WEST_COVER_DRIFT`].
const WEST_BAND: f64 = 7.0;
const WEST_COVER: (f64, f64) = (0.14, 0.34);
const WEST_COVER_DRIFT: f64 = 0.4;
/// The scrub down the slope behind Gaviota's south row toward Gardenia, under its trees:
/// darker olive and green, a little taller, kept off the lane along the level's south
/// wall ([`SOUTH_LANE`], as the band of oaks is).
const SOUTH_MAT: (f64, f64) = (0.4, 1.0);
const SOUTH_MAT_TONES: [[f32; 3]; 3] = [[0.12, 0.31, 0.33], [0.15, 0.36, 0.4], [0.1, 0.25, 0.26]];
const SOUTH_MAT_SHADE: [f32; 3] = [0.04, 0.09, 0.07];
/// Clumps of big silver-grey agaves standing out of the west slope's mat: a clump every so
/// many metres on a jittered grid (some sites left bare), of this many agaves at least,
/// this size range (m).
const WEST_AGAVE_SPACING: f64 = 14.0;
const WEST_AGAVES: usize = 5;
const WEST_AGAVE: (f64, f64) = (0.45, 0.7);
const WEST_AGAVE_LEAVES: u32 = 0x7f8b88;
/// The bigger shrubs standing out of the west slope's mat: one every so many metres on a
/// jittered grid (some sites left bare), their size range (m) and leaves: grey-green
/// rosemary and sage.
const WEST_SHRUB_SPACING: f64 = 1.9;
const WEST_SHRUB: (f64, f64) = (0.5, 0.8);
const WEST_SHRUB_LEAVES: [u32; 4] = [0x4e5a3c, 0x5a6447, 0x46523a, 0x626b4f];
/// Olive trees along the foot of Neblina's west row's rear walls, one behind each lot,
/// this far (m) out from the wall: grey-green crowns (breadth and height range, m) on
/// twisted dark trunks.
const OLIVE_OUT: f64 = 2.2;
/// The belt of canopy behind Neblina's west row: its two lines this far (m) out from the
/// rear walls, a crown every this many metres along each, staggered.
const WEST_CANOPY_REACH: [f64; 2] = [OLIVE_OUT, 5.0];
const WEST_CANOPY_STEP: f64 = 3.8;
const OLIVE_CROWN: (f64, f64) = (2.4, 3.2);
const OLIVE_HEIGHT: (f64, f64) = (3.0, 3.8);
const OLIVE_LEAVES: [u32; 3] = [0x5c6450, 0x535b48, 0x646b56];
const OLIVE_BARK: u32 = 0x4a4036;
/// The band of tree crowns down the slope behind Gaviota's south row, between the cover
/// oaks: two staggered lines this far (m) out past the back walls (the strip to the
/// level's wall is about 7.5 m, its outer [`SOUTH_LANE`] kept for tanks), a crown every
/// this many metres along each, none nearer a cover tree's trunk than this (m).
const SOUTH_CANOPY_REACH: [f64; 2] = [2.0, 3.0];
const SOUTH_CANOPY_STEP: f64 = 4.0;
const SOUTH_CANOPY_ROOM: f64 = 2.4;
/// Small trees along the foot of Rumbo's north row's rear fences on the bank, one where
/// each pair of neighbours' lot lines meet, by turns this far (m) out from the fence; their
/// crowns' breadth and heights (m); none within this far (m) of a pickup.
const BANK_TOE_REACH: [f64; 2] = [1.6, 2.6];
const BANK_TOE_CROWN: (f64, f64) = (3.0, 3.8);
const BANK_TOE_HEIGHT: (f64, f64) = (4.0, 5.0);
const BANK_TOE_PICKUP_ROOM: f64 = 3.0;
/// The small trees inside the level's west wall, between the spawn pads (scenery): this
/// far (m) inside the wall's face, their crowns' breadth and heights (m), two to a gap
/// between pads.
const WEST_TREE_INSET: f64 = 2.8;
const WEST_TREE_CROWN: (f64, f64) = (2.2, 3.0);
const WEST_TREE_HEIGHT: (f64, f64) = (3.8, 4.8);
/// The hillside's scrub gathers in patches where a slow noise passes this level.
const HILL_PATCH: f64 = 0.35;
/// The hillside's coastal sage inside the arena: a clump every this many metres.
const HILL_SAGE_SPACING: f64 = 2.0;
/// Past the arena's edge, where it only closes the view, a little sparser.
const OUTER_SAGE_SPACING: f64 = 2.6;
const GRASS_SPACING: f64 = 1.2;
const GRASS_LEAVES: [u32; 2] = [0x86825a, 0x787a50];
const GRASS_LUMP: (f64, f64) = (0.3, 0.5);
/// How far (m) the shrubs keep from the paving, a spawn pad's middle and a pickup.
const SHRUB_PAVING_ROOM: f64 = 0.4;
const SHRUB_PAD_ROOM: f64 = 3.6;
const SHRUB_PICKUP_ROOM: f64 = 1.6;
/// The planted slopes' shrubs reach this far (m) from the arena's centre either way.
const SHRUB_EXTENT: f64 = 80.0;
/// The ornamental grass beds on the corners where Rumbo and the greenbelt meet
/// Neblina and where Neblina leaves Senda Parkway: how far (m) round each corner they
/// reach.
const CORNER_BED: f64 = 3.2;
/// The agaves on the corner wedge north of Rumbo's mouth: how far along Rumbo past
/// Neblina's sidewalk and how far out past its own (m).
const CORNER_AGAVES: [(f64, f64); 4] = [(0.6, 0.9), (1.4, 1.8), (0.9, 2.6), (1.9, 0.8)];
/// The bend park's cluster mailboxes: how many units, each unit's cabinet (width, height,
/// depth, m) on its pedestal (height, m), and their bronze-green paint and the concrete
/// pad under them.
const MAILBOX_UNITS: usize = 4;
const MAILBOX: (f64, f64, f64) = (0.42, 0.95, 0.42);
const MAILBOX_PEDESTAL: f64 = 0.45;
const MAILBOX_PAINT: u32 = 0x4a554c;
const MAILBOX_PAD: u32 = 0xc4bfb3;
/// Each house's entry bed beside its front walk, on the side away from the driveway, at
/// the street end of the front yard: its size along the front and out from it (m), the
/// low stucco seat wall in the house's own stucco along its street side and down the
/// walk (height, thickness, m), the capped pilasters at its ends (side and height, m),
/// the cap's paint and the bed's mulch.
const ENTRY_BED: (f64, f64) = (1.3, 1.0);
const SEAT_WALL: (f64, f64) = (0.35, 0.18);
const SEAT_PILASTER: (f64, f64) = (0.3, 0.45);
/// What an entry bed holds, by spot (`u` from the walk's far end toward it and `v` out to
/// the street, as shares of the bed) and kind: mounded shrubs of three sizes (radius,
/// m), a red-flowering shrub and an agave.
const ENTRY_PLANTS: [(f64, f64, EntryPlant); 5] = [
    (-0.3, -0.22, EntryPlant::Shrub(0.36)),
    (0.08, 0.1, EntryPlant::Shrub(0.26)),
    (0.32, -0.25, EntryPlant::Shrub(0.2)),
    (-0.05, -0.3, EntryPlant::Flowering),
    (-0.32, 0.15, EntryPlant::Agave),
];
const ENTRY_LEAVES: [u32; 3] = [0x4a5e3a, 0x5f6a45, 0x55603f];
const SEAT_CAP: (f64, u32) = (0.05, 0xd9cfbd);
const BED_MULCH: u32 = 0x5c4a36;
/// The bank's crest beside Senda Parkway's path: a row of dark mulch basins, half discs
/// this wide (radius, m), one every this many metres, each holding a shrub.
const CREST_BASIN: (f64, f64, u32) = (0.9, 4.8, 0x4a3b2c);
/// The agaves and the concrete down-drain at the end of each cul-de-sac on the hillside:
/// how far (m) past the turning circle's sidewalk, the agaves' blue-grey leaves and how
/// many stand in a cluster.
const CUL_DE_SAC_END: f64 = 9.0;
const AGAVE: u32 = 0x7d9089;
/// An agave's rings of leaves: how many, how steeply they rise (radians), how long for
/// the plant's size and how far round each ring is turned (a share of a leaf's step).
const AGAVE_RINGS: [(usize, f64, f64, f64); 2] = [(8, 0.5, 1.0, 0.0), (5, 1.05, 0.8, 0.5)];
const AGAVES: usize = 5;

/// Senda's scenery, retained across matches.
pub fn senda_scenery() -> Node {
    let mut root = Node::group("senda-scenery");
    root.children.push(grass());
    root.children.push(planted_ground(&Lines::new()));
    root.children.push(paving());
    let mut pads = Node::group("");
    pads.children.push(create_spawn_pads(1.0));
    batch(&mut pads);
    root.children.push(pads);
    root.children.push(slope_trees());
    root.children.push(greenbelt_scrub());
    root.children.push(outer_slopes());
    root.children.push(shrubs());
    root.children.push(bank_planting());
    root.children.push(yard_props());
    root.children.push(street_lamps());
    root
}

/// The ground: dry-grass ground watered green over the lawns, parkways and the
/// greenbelt, planted beds in the front yards, groundcover along Senda Parkway, left as
/// sage scrub on the slopes, rising past the arena east and south into the green
/// rolling hills every street of the neighbourhood looks out on.
fn grass() -> Node {
    let segments = (GROUND_EXTENT / 2.5).round() as u32;
    let mut geometry = plane_geometry_segments(GROUND_EXTENT, GROUND_EXTENT, segments, segments);
    geometry.rotate_x(-FRAC_PI_2);
    for p in &mut geometry.positions {
        p[1] = hill_height(f64::from(p[0]), f64::from(p[2])) as f32;
    }
    geometry.compute_vertex_normals();
    ground_uvs(&mut geometry, 0.0, 0.0);
    let lines = Lines::new();
    geometry.colors = geometry
        .positions
        .iter()
        .map(|p| grass_tint(DVec2::new(f64::from(p[0]), f64::from(p[2])), &lines))
        .collect();
    let mut surface = ground_material(GroundKind::DryGrass);
    surface.vertex_colors = true;
    surface.color = crate::scene::Color(0xffffff);
    let mut floor = shadow_receiver(Arc::new(geometry), Arc::new(surface));
    floor.position.y = 0.008;
    floor
}

/// The planted ground laid over the grass: bark mulch with groundcover drifting through
/// it on the bank below Senda Parkway and across it, in the field behind Neblina's west
/// row, the slope behind Gaviota's south row and the greenbelt's mouth, and dark mulch in
/// the front yards' beds; everywhere but the lots' yards, the greenbelt's scrub, the
/// hillside's natural grass, the hills and the paving. Square cells of a fine grid where
/// it lies, the packed-dirt tile under vertex tints, so the planted slopes read as mulch
/// and groundcover rather than as mown grass; their ragged edges fall under the walls,
/// the slabs and the scrub.
fn planted_ground(lines: &Lines) -> Node {
    let reach = ARENA + PLANTED_REACH;
    let cells = (2.0 * reach / PLANTED_STEP).round() as usize;
    let corner = |i: usize, j: usize| {
        DVec2::new(
            -reach + i as f64 * PLANTED_STEP,
            -reach + j as f64 * PLANTED_STEP,
        )
    };
    let planted = |p: DVec2| {
        let zone = zones(p, lines);
        // Outside the level's wall only the bank under Senda Parkway is planted: the
        // natural grass runs up to the wall everywhere else.
        let open = (1.0 - zone.lawn).max(zone.front)
            * (1.0 - (zone.greenbelt - zone.mouth).max(0.0))
            * (1.0 - zone.hillside)
            * (1.0 - zone.hills)
            * (1.0 - zone.beyond_wall);
        open > 0.5 && !is_senda_paved(p.x, p.y)
    };
    let mut index = vec![u32::MAX; (cells + 1) * (cells + 1)];
    let mut positions: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for j in 0..cells {
        for i in 0..cells {
            let middle = corner(i, j) + DVec2::splat(PLANTED_STEP / 2.0);
            if !planted(middle) {
                continue;
            }
            let quad = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)].map(|(i, j)| {
                let k = j * (cells + 1) + i;
                if index[k] == u32::MAX {
                    let p = corner(i, j);
                    positions.extend([p.x, 0.0, p.y]);
                    colors.push(planted_tint(p, lines));
                    index[k] = (positions.len() / 3 - 1) as u32;
                }
                index[k]
            });
            let [a, b, c, d] = quad;
            indices.extend([a, d, c, a, c, b]);
        }
    }
    let count = positions.len() / 3;
    let normals: Vec<f64> = (0..count).flat_map(|_| [0.0, 1.0, 0.0]).collect();
    let uvs = vec![0.0; count * 2];
    let mut mesh = Mesh::from_f64(&positions, &normals, &uvs, Some(indices));
    ground_uvs(&mut mesh, 0.0, 0.0);
    mesh.colors = colors;
    let mut surface = ground_material(GroundKind::PackedDirt);
    surface.vertex_colors = true;
    surface.color = crate::scene::Color(0xffffff);
    let mut node = shadow_receiver(Arc::new(mesh), Arc::new(surface));
    node.position.y = 0.014;
    node
}

/// The planted ground's tint at `p` over the packed-dirt tile, linear: bark mulch with
/// groundcover drifting through it (more of it down the bank and round the tract), the
/// shaded groundcover of the slope behind Gaviota's south row and of the bank's foot,
/// and the front beds' dark mulch.
fn planted_tint(p: DVec2, lines: &Lines) -> [f32; 3] {
    let zone = zones(p, lines);
    let noise = 0.5
        + 0.18 * (p.x * 0.53 + p.y * 0.71).sin()
        + 0.16 * (p.x * 0.67 - p.y * 0.37).cos()
        + 0.12 * (p.y * 1.13 - p.x * 0.29 + 1.7).sin();
    let (least, most) = if zone.bank_land > 0.5 {
        BANK_PLANTED
    } else {
        SLOPE_PLANTED
    };
    let cover = (least + (most - least) * noise.clamp(0.0, 1.0)) * (1.0 - zone.front);
    let mix = |a: f32, b: f32, t: f64| a + (b - a) * t as f32;
    std::array::from_fn(|i| {
        let planted = mix(DIRT_MULCH[i], DIRT_COVER[i], cover);
        // Under the slopes' trees the groundcover is darker, the mulch showing through.
        let shaded = mix(DIRT_MULCH[i] * 0.8, DIRT_SHADE[i], 0.45 + 0.55 * noise);
        let planted = mix(planted, shaded, zone.bank_foot * (1.0 - zone.bank_land));
        // Behind the outer rows, under the street trees and the slope's: groundcover
        // over the mulch, darker in the shade.
        let under_trees = mix(
            DIRT_SHADE[i],
            DIRT_COVER[i],
            UNDER_TREES_COVER.0 + UNDER_TREES_COVER.1 * noise,
        );
        // Past the band of trees behind Neblina's west row, the field is grey-brown
        // mulch with drifts of low groundcover.
        let field = mix(
            DIRT_BANK_MULCH[i] * (0.62 + 0.16 * noise) as f32,
            DIRT_COVER[i],
            0.3 * noise,
        );
        let west = mix(field, under_trees, west_band(p, lines));
        let planted = mix(planted, under_trees, zone.south);
        let planted = mix(planted, west, zone.west);
        // The greenbelt's mouth: blue-grey and green groundcover under its grasses.
        let mouth = mix(MOUTH_COVER[i], DIRT_COVER[i], 0.3 + 0.4 * noise);
        let planted = mix(planted, mouth, zone.mouth);
        // The bank: grey-brown bark mulch under its contour rows of shrubs, and the dark
        // underside of its groundcover carpet over the upper band.
        let carpet = rise((BANK_COVER_DOWN + 0.03 - zone.bank_fall) / 0.05);
        let mulch = DIRT_BANK_MULCH[i] * (0.92 + 0.16 * noise) as f32;
        let bank = mix(mulch, BANK_MAT_SHADE[i], carpet);
        let planted = mix(planted, bank, zone.bank_land);
        mix(planted, DIRT_BED[i], zone.front)
    })
}

/// The hills past the arena: they rise from this far out on every side (arena x and z,
/// clear of the streets that run on past the edge), to about this height (m) over this
/// many metres, as long, low ridges rolling along their crest over these lengths, so
/// every street ends in hills a few degrees over the rooftops, as the neighbourhood's
/// views do.
const HILLS_FROM: f64 = 80.0;
const HILLS_HEIGHT: f64 = 15.0;
const HILLS_RISE: f64 = 65.0;
const HILLS_ROLL: (f64, f64) = (230.0, 140.0);

/// The ground's height at `(x, z)`: level over the neighbourhood, rising past
/// [`HILLS_FROM`] all round into long rolling ridges.
fn hill_height(x: f64, z: f64) -> f64 {
    let reach = (x.abs().max(z.abs()) - HILLS_FROM) / HILLS_RISE;
    if reach <= 0.0 {
        return 0.0;
    }
    let t = reach.min(1.0);
    let rise = t * t * (3.0 - 2.0 * t);
    // Along the ring: the angle round the arena, as a distance on a 150 m circle.
    let along = z.atan2(x) * 150.0;
    let roll = 0.72
        + 0.2 * (along * std::f64::consts::TAU / HILLS_ROLL.0).sin()
        + 0.1 * (along * std::f64::consts::TAU / HILLS_ROLL.1 + 1.1).cos();
    HILLS_HEIGHT * rise * roll
}

/// The street lines and lots the ground's tint follows.
struct Lines {
    rumbo: Line,
    gaviota: Line,
    way: Line,
    /// How far along Neblina it bends into Gaviota: past it the slope south of
    /// Gaviota's south row runs down toward Gardenia, before it the field behind
    /// Neblina's west row.
    bend: f64,
    /// How far along Neblina the greenbelt's mouth ends, at Gaviota's first lot.
    mouth_end: f64,
    /// The greenbelt's east end, at the nearer cul-de-sac (arena x): past it the
    /// hillside runs on.
    greenbelt_end: f64,
    lots: Vec<Lot>,
    /// The level's wall: past it the ground is the hillside's natural grass.
    boundary: Vec<DVec2>,
}

impl Lines {
    fn new() -> Self {
        let gaviota = Street::NeblinaGaviota.line();
        let lots = senda_lots(&senda_houses());
        // The mouth runs on along Neblina to the side wall of Gaviota's first lot.
        let first_lot = lots
            .iter()
            .filter(|lot| lot.house.row == Row::GaviotaNorth)
            .flat_map(|lot| {
                let (u, v) = (lot.half_width, lot.half_depth + lot.house.setback);
                [lot.at(-u, v), lot.at(u, v)]
            })
            .map(|corner| gaviota.along(corner))
            .fold(f64::INFINITY, f64::min);
        let mouth_end = if first_lot.is_finite() {
            first_lot
        } else {
            greenbelt_mouth() - HOUSE_WIDTH / 2.0 - SIDE_YARD
        };
        Lines {
            rumbo: Street::Rumbo.line(),
            gaviota,
            way: Street::SendaParkway.line(),
            bend: neblina_bend(),
            mouth_end,
            greenbelt_end: Street::ALL
                .into_iter()
                .filter_map(Street::cul_de_sac)
                .map(|middle| middle.x)
                .fold(f64::INFINITY, f64::min),
            lots,
            boundary: senda_boundary(),
        }
    }

    /// How far `p` lies inside the lot it is deepest in (negative outside them all), and
    /// how far in front of that lot's house.
    fn lot(&self, p: DVec2) -> (f64, f64) {
        self.lots
            .iter()
            .map(|lot| (lot.inside(p), lot.local(p).1 - lot.half_depth))
            .fold((f64::NEG_INFINITY, 0.0), |best, lot| {
                if lot.0 > best.0 { lot } else { best }
            })
    }

    /// How far (m) out from `street`'s line at `s` metres along it, to its left
    /// (`side` 1) or right (-1), the back walls of its lots run: the first spot past its
    /// houses that lies outside every lot.
    fn back_walls(&self, street: &Line, s: f64, side: f64) -> f64 {
        let (point, along) = street.at(s);
        let out = left(along) * side;
        let mut reach = house_front() + HOUSE_DEPTH;
        while reach < ARENA && self.lot(point + out * reach).0 > -0.05 {
            reach += 0.1;
        }
        reach
    }
}

/// How much of each kind of ground lies at a spot, each from 0 to 1 over a metre or so
/// of its edge.
struct Zones {
    /// The lots either side of Rumbo, Neblina and Gaviota out to their back walls.
    lawn: f64,
    /// The lots' front yards, between the sidewalk and the houses' fronts: planted beds
    /// of mulch and groundcover, as the tract's are, not lawn.
    front: f64,
    /// Between Rumbo's south row and Gaviota's north row, and at its mouth on Neblina
    /// between their corner lots (the mouth on its own too).
    greenbelt: f64,
    mouth: f64,
    /// The groundcover strip along Senda Parkway's kerbs.
    groundcover: f64,
    /// Down the bank from Senda Parkway's path to Rumbo's back walls: how far down it
    /// (its shade), the bank itself, and its lower third.
    bank: f64,
    bank_land: f64,
    bank_foot: f64,
    /// How far down the bank a spot lies, from 0 at the Way's path to 1 at the back walls.
    bank_fall: f64,
    /// The planted field behind Neblina's west row, and the slope behind Gaviota's
    /// south row.
    west: f64,
    south: f64,
    /// The natural hillside east of the cul-de-sacs.
    hillside: f64,
    /// Outside the level's wall, but for the bank under Senda Parkway north of it:
    /// natural grass.
    beyond_wall: f64,
    /// The green hills past the arena east and south.
    hills: f64,
}

fn zones(p: DVec2, lines: &Lines) -> Zones {
    // Past the houses' back walls.
    let rows = house_front() + HOUSE_DEPTH;
    let (rumbo, gaviota) = (
        lines.rumbo.signed_distance(p),
        lines.gaviota.signed_distance(p),
    );
    let (inside, ahead) = lines.lot(p);
    // No lots across Senda Parkway (its left), where Neblina's line ends.
    let south_of_way = (-lines.way.signed_distance(p)).clamp(0.0, 1.0);
    let way = lines.way.distance(p) - Street::SendaParkway.half();
    // South of Rumbo (its right) and north of Gaviota (its left).
    let between = ((-rumbo).min(gaviota) - rows + 1.0).clamp(0.0, 1.0)
        * ((lines.greenbelt_end - p.x) / HILLSIDE_BLEND).clamp(0.0, 1.0);
    // Past Rumbo's lots on Neblina's east side, short of Gaviota's first lot.
    let mouth = ((-rumbo) - rows + 1.0).clamp(0.0, 1.0)
        * (gaviota - STREET_HALF_WALK).clamp(0.0, 1.0)
        * (lines.mouth_end - lines.gaviota.along(p)).clamp(0.0, 1.0);
    let lawn = (inside + 0.5).clamp(0.0, 1.0) * south_of_way * (1.0 - mouth);
    // Neblina's west side and Gaviota's south side (Neblina's line's right), past
    // their rows: the field before the bend, the slope past it.
    let outer = ((-gaviota) - rows + 1.0).clamp(0.0, 1.0) * (1.0 - lawn) * south_of_way;
    let south =
        outer * ((lines.gaviota.along(p) - lines.bend) / HILLSIDE_BLEND + 0.5).clamp(0.0, 1.0);
    let down = way - WAY_PARKWAY - WAY_PATH;
    let bank_land = down.clamp(0.0, 1.0) * (rumbo - rows).clamp(0.0, 1.0) * south_of_way;
    // The share of the way down from the bank's top to the back walls.
    let fall = down / (down + (-inside).max(0.0)).max(0.1);
    Zones {
        lawn,
        front: lawn * (ahead + FRONT_BEDS_REACH).clamp(0.0, 1.0),
        greenbelt: between.max(mouth) * (1.0 - lawn),
        mouth: mouth * (1.0 - lawn),
        groundcover: (WAY_PARKWAY + 0.5 - way).clamp(0.0, 1.0),
        bank: (down / BANK_FALL).clamp(0.0, 1.0) * (rumbo - rows).clamp(0.0, 1.0) * south_of_way,
        bank_land,
        bank_foot: bank_land * ((fall - 0.62) / 0.08).clamp(0.0, 1.0) * (1.0 - lawn),
        bank_fall: fall,
        west: outer - south,
        south,
        hillside: ((p.x - HILLSIDE_FROM) / HILLSIDE_BLEND).clamp(0.0, 1.0),
        // The bank runs on outside the north wall under the Way; west of the level the
        // Way has ended at the wall.
        beyond_wall: if inside_boundary(&lines.boundary, p) {
            0.0
        } else if p.y < -ARENA {
            1.0 - bank_land
        } else {
            1.0
        },
        hills: ((p.x.abs().max(p.y.abs()) - HILLS_FROM) / HILLSIDE_BLEND).clamp(0.0, 1.0),
    }
}

/// The hillside's scrub patches: where a slow noise passes [`HILL_PATCH`], from 0 to 1.
fn hill_patch(p: DVec2) -> f64 {
    let noise =
        0.5 + 0.25 * (p.x * 0.19 + p.y * 0.27).sin() + 0.2 * (p.y * 0.23 - p.x * 0.13 + 0.9).cos();
    ((noise - HILL_PATCH) / 0.12).clamp(0.0, 1.0)
}

/// The ground's colour at `p`: the planted slopes' groundcover drifting over bark mulch
/// (darkening down the bank below Senda Parkway, darkest on its lower third), the shaded
/// groundcover of the slope behind Gaviota's south row, the greenbelt's dark olive under
/// its scrub, the groundcover along Senda Parkway, the lots' small lawns and their front
/// beds, and east of the cul-de-sacs the hillside's natural grass, green with the winter
/// rains and tan between, with its darker patches of scrub.
fn grass_tint(p: DVec2, lines: &Lines) -> [f32; 3] {
    let zone = zones(p, lines);
    // Slow enough for the ground's 2.5 m vertices to follow, in three directions so
    // the drifts do not line up in waves.
    let noise = 0.5
        + 0.16 * (p.x * 0.31 + p.y * 0.52).sin()
        + 0.16 * (p.x * 0.47 - p.y * 0.23).cos()
        + 0.12 * (p.y * 0.41 - p.x * 0.11 + 1.7).sin();
    let patch = 0.5 + 0.5 * ((p.x * 0.21 + p.y * 0.13).sin() * (p.y * 0.17 - p.x * 0.08).cos());
    // The greenbelt's ground in the shade of its scrub, a few metres across.
    let mottle = (0.5
        + 0.3 * (p.x * 0.83 + p.y * 0.41).sin()
        + 0.25 * (p.y * 0.97 - p.x * 0.29 + 0.6).cos())
    .clamp(0.0, 1.0);
    // The hillside's green and tan, about 8 m across.
    let tan = (0.5 + 0.5 * (p.x * 0.61 + p.y * 0.47).sin() * (p.y * 0.53 - p.x * 0.29).cos())
        .clamp(0.0, 1.0);
    let scrub = hill_patch(p);
    let mix = |a: f32, b: f32, t: f64| a + (b - a) * t as f32;
    std::array::from_fn(|i| {
        let (least, most) = (
            mix(
                SLOPE_PLANTED.0 as f32,
                BANK_PLANTED.0 as f32,
                zone.bank_land,
            ),
            mix(
                SLOPE_PLANTED.1 as f32,
                BANK_PLANTED.1 as f32,
                zone.bank_land,
            ),
        );
        let cover = least + (most - least) * noise.clamp(0.0, 1.0) as f32;
        // The bank's groundcover is greyer than the slopes' round the tract.
        let green = mix(GROUNDCOVER[i], BANK_COVER[i], zone.bank_land);
        let planted = mix(MULCH[i], green, f64::from(cover));
        let planted = mix(planted, SHADED_COVER[i], zone.south);
        let planted = mix(planted, BANK_FOOT[i], zone.bank_foot);
        // Mostly tan-olive sage scrub, the winter grass green only in patches.
        let grass = mix(
            HILLS[i],
            HILLSIDE_TAN[i],
            HILLSIDE_DRY + tan * (1.0 - HILLSIDE_DRY),
        );
        let natural = mix(
            grass,
            SAGE[i] * 0.75,
            HILLSIDE_SCRUB + scrub * (0.9 - HILLSIDE_SCRUB),
        );
        let wild = zone.hillside.max(zone.beyond_wall);
        let slope = mix(planted, natural, wild) * (1.0 - BANK_SHADE * zone.bank as f32);
        let greenbelt = mix(GREENBELT_TONES[2][i], GREENBELT_TONES[0][i], mottle * 0.6);
        let greenbelt = mix(greenbelt, GROUNDCOVER[i], zone.mouth);
        let tint = mix(slope, greenbelt, zone.greenbelt);
        let tint = mix(tint, GROUNDCOVER[i], zone.groundcover);
        let tint = mix(tint, LAWN[i], zone.lawn);
        let beds = mix(
            BED_TINT[i],
            GROUNDCOVER[i],
            FRONT_BEDS_COVER * (0.6 + 0.8 * patch),
        );
        let tint = mix(tint, beds, zone.front);
        // Tan patches over the hills' green, on a slower noise of their own.
        let dry = (0.5
            + 0.5 * (p.x * 0.043 + p.y * 0.031).sin() * (p.y * 0.052 - p.x * 0.027).cos())
        .clamp(0.0, 1.0);
        let hills = mix(HILLS[i], HILLS_DRY[i], (dry - 0.45).clamp(0.0, 0.5) * 1.6);
        let tint = mix(tint, mix(hills, SAGE[i], scrub * 0.3), zone.hills);
        tint * (0.94 + 0.06 * patch as f32)
    })
}

/// A flat ribbon along `line` between `from` and `to` metres to its left, its top at
/// `y`, leaving out the stretches `skip` says lie on something else (another street's
/// asphalt, a turning circle).
fn ribbon(line: &[DVec2], from: f64, to: f64, y: f64, skip: &dyn Fn(DVec2) -> bool) -> Mesh {
    let n = line.len();
    let normal = |i: usize| {
        let along = (line[(i + 1).min(n - 1)] - line[i.saturating_sub(1)]).normalize();
        left(along)
    };
    let mut positions: Vec<f64> = Vec::new();
    let mut normals: Vec<f64> = Vec::new();
    // World-aligned, like every other part it is batched with: a merge of parts whose
    // attributes differ draws nothing.
    let mut uvs: Vec<f64> = Vec::new();
    for i in 0..n - 1 {
        let (a, b) = (line[i], line[i + 1]);
        let (na, nb) = (normal(i), normal(i + 1));
        // Split long spans so the skipped stretches end near what they meet.
        let pieces = (a.distance(b) / 1.0).ceil().max(1.0) as usize;
        for k in 0..pieces {
            let (t0, t1) = (k as f64 / pieces as f64, (k + 1) as f64 / pieces as f64);
            let at = |t: f64, offset: f64| {
                let point = a.lerp(b, t);
                point + na.lerp(nb, t).normalize() * offset
            };
            let quad = [at(t0, from), at(t1, from), at(t1, to), at(t0, to)];
            let middle = (quad[0] + quad[2]) / 2.0;
            if skip(middle) {
                continue;
            }
            // Wound so its face points up whichever side of the line it lies.
            let order = if to > from {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 2, 1, 0, 3, 2]
            };
            for index in order {
                positions.extend([quad[index].x, y, quad[index].y]);
                normals.extend([0.0, 1.0, 0.0]);
                uvs.extend([quad[index].x / 4.0, quad[index].y / 4.0]);
            }
        }
    }
    Mesh::from_f64(&positions, &normals, &uvs, None)
}

/// A painted, shadow-receiving flat piece.
fn flat(mesh: Mesh, color: u32) -> Node {
    let mut node = shadow_receiver(Arc::new(mesh), paint(color));
    paint_mesh(&mut node);
    node
}

/// Every paved surface, batched into one draw per material.
fn paving() -> Node {
    let mut group = Node::group("senda-paving");
    let streets: Vec<(Street, Vec<DVec2>)> = Street::ALL
        .into_iter()
        .map(|street| (street, street.line().0))
        .collect();
    let lines: Vec<(Street, crate::sim::senda_layout::Line)> = Street::ALL
        .into_iter()
        .map(|street| (street, street.line()))
        .collect();
    let bulbs: Vec<DVec2> = Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .collect();
    for (street, line) in &streets {
        let half = street.half();
        // The asphalt runs on into the junctions; kerbs and sidewalks stop at the
        // asphalt of the street they meet and round the turning circles.
        let crossing = |p: DVec2| {
            lines
                .iter()
                .any(|(other, line)| other != street && line.distance(p) < other.half() + 0.1)
                || bulbs
                    .iter()
                    .any(|b| b.distance(p) < CUL_DE_SAC_RADIUS + 0.1)
        };
        let nothing = |_: DVec2| false;
        group.children.push(flat(
            ribbon(line, -half, half, STREET_TOP, &nothing),
            ASPHALT,
        ));
        for side in [-1.0, 1.0] {
            let kerb = ribbon(
                line,
                side * half,
                side * (half + KERB_WIDTH),
                KERB_HEIGHT,
                &crossing,
            );
            group.children.push(flat(kerb, KERB));
            // Senda Parkway's south side has the wide path past its groundcover.
            let (near, far) = if *street == Street::SendaParkway && side < 0.0 {
                (half + WAY_PARKWAY, half + WAY_PARKWAY + WAY_PATH)
            } else {
                (half + PARKWAY, half + PARKWAY + SIDEWALK)
            };
            let walk = ribbon(line, side * near, side * far, WALK_TOP, &crossing);
            group.children.push(flat(walk, SIDEWALK_CONCRETE));
        }
        if *street == Street::SendaParkway {
            senda_parkway(&mut group, line);
        }
    }
    for middle in &bulbs {
        cul_de_sac(&mut group, *middle);
    }
    let lines = Lines::new();
    neblina_centre_line(&mut group);
    splitter_island(&mut group);
    greenbelt_path(&mut group, &lines);
    trails(&mut group, &lines);
    let houses = senda_houses();
    yard_grounds(&mut group, &lines.lots);
    front_lawns(&mut group, &lines.lots);
    bend_park_grounds(&mut group, &lines);
    crest_basins(&mut group, &lines);
    for house in &houses {
        let design = house.design();
        driveway(&mut group, &design, house);
    }
    entry_bed_beds(&mut group, &houses);
    batch(&mut group);
    group
}

/// Neblina's double yellow centre line, from Senda Parkway's kerb to the bend.
fn neblina_centre_line(group: &mut Node) {
    let (line, way) = (Street::NeblinaGaviota.line(), Street::SendaParkway.line());
    let bend = neblina_bend();
    let (island, _) = senda_layout::splitter_island();
    let skip = |p: DVec2| {
        way.distance(p) < Street::SendaParkway.half()
            || line.along(p) > bend
            || line.along(p) < island
    };
    for side in [-1.0, 1.0] {
        let (near, far) = (
            side * CENTRE_LINE_GAP / 2.0,
            side * (CENTRE_LINE_GAP / 2.0 + CENTRE_LINE),
        );
        let paint = ribbon(&line.0, near, far, STREET_TOP + 0.006, &skip);
        group.children.push(flat(paint, CENTRE_LINE_PAINT));
    }
}

/// The kerbed splitter island in Neblina's mouth, planted with groundcover: round at its
/// end toward Senda Parkway, tapering to a point down Neblina's centre line, in pieces
/// this long (m).
const ISLAND_PIECE: f64 = 0.25;

fn splitter_island(group: &mut Node) {
    let neblina = Street::NeblinaGaviota.line();
    let (from, to) = senda_layout::splitter_island();
    let (_, _, width) = SPLITTER_ISLAND;
    let nose = width / 2.0;
    // Half the island's width `t` metres down it: a half disc for its nose, then a
    // straight taper.
    let half = |t: f64| {
        if t < nose {
            (nose * nose - (nose - t) * (nose - t)).max(0.0).sqrt()
        } else {
            nose * (1.0 - (t - nose) / (to - from - nose)).max(0.0)
        }
    };
    let pieces = ((to - from) / ISLAND_PIECE).ceil() as usize;
    for (grow, y, color) in [
        (KERB_WIDTH, KERB_HEIGHT, KERB),
        (0.0, KERB_HEIGHT + 0.01, MEDIAN),
    ] {
        for k in 0..pieces {
            let t0 = k as f64 * ISLAND_PIECE;
            let t1 = (t0 + ISLAND_PIECE).min(to - from);
            let (a, along) = neblina.at(from + t0);
            let (b, _) = neblina.at(from + t1);
            let side = left(along);
            let (h0, h1) = (half(t0) + grow, half(t1) + grow);
            let corners = [a - side * h0, b - side * h1, b + side * h1, a + side * h0];
            group.children.push(flat(quad(corners, y), color));
        }
    }
}

/// Senda Parkway's raised median, kerbed, planted and mulched under its trees, and its
/// bike lanes' solid lines. The median opens across Neblina's mouth for the turn into it,
/// each side of the opening ending in a round nose its kerb wraps.
fn senda_parkway(group: &mut Node, line: &[DVec2]) {
    let nothing = |_: DVec2| false;
    let top = KERB_HEIGHT + 0.01;
    let way = Line(line.to_vec());
    let reach = MEDIAN_HALF + KERB_WIDTH;
    for (from, to) in median_spans(&way) {
        let span = cut_line(&way, from + reach, to - reach);
        if span.len() < 2 {
            continue;
        }
        group.children.push(flat(
            ribbon(&span, -MEDIAN_HALF, MEDIAN_HALF, top, &nothing),
            MEDIAN,
        ));
        for side in [-1.0, 1.0] {
            let kerb = ribbon(
                &span,
                side * MEDIAN_HALF,
                side * reach,
                KERB_HEIGHT,
                &nothing,
            );
            group.children.push(flat(kerb, KERB));
        }
        // A nose where the median meets the opening, not where it runs off the level.
        for (end, outward) in [(from, -1.0), (to, 1.0)] {
            if end <= 0.0 || end >= way.length() {
                continue;
            }
            let (centre, along) = way.at(end - outward * reach);
            let out = along * outward;
            group
                .children
                .push(flat(nose(centre, out, (0.0, MEDIAN_HALF), top), MEDIAN));
            group.children.push(flat(
                nose(centre, out, (MEDIAN_HALF, reach), KERB_HEIGHT),
                KERB,
            ));
        }
    }
    let half = Street::SendaParkway.half();
    let middle = half - BIKE_LANE;
    let (a, b) = (middle - LANE_LINE / 2.0, middle + LANE_LINE / 2.0);
    for side in [-1.0, 1.0] {
        group.children.push(flat(
            ribbon(line, side * a, side * b, STREET_TOP + 0.006, &nothing),
            LANE_PAINT,
        ));
    }
}

/// Gaviota's north row, the lots that back onto the greenbelt from its south side, in
/// order along the street.
fn gaviota_north_row(lines: &Lines) -> Vec<&Lot> {
    let rows = house_front() + HOUSE_DEPTH;
    let mut row: Vec<&Lot> = lines
        .lots
        .iter()
        .filter(|lot| {
            let gaviota = lines.gaviota.signed_distance(lot.centre);
            gaviota > 0.0 && gaviota < rows && lines.rumbo.signed_distance(lot.centre) < -rows
        })
        .collect();
    row.sort_by(|a, b| {
        lines
            .gaviota
            .along(a.centre)
            .total_cmp(&lines.gaviota.along(b.centre))
    });
    row
}

/// The middle of the greenbelt's mouth on Neblina's sidewalk, where its path comes in
/// from the street.
fn mouth_entry(lines: &Lines) -> Option<DVec2> {
    let entry = STREET_HALF_WALK + 0.2;
    let mut span: Vec<DVec2> = Vec::new();
    let mut s = (lines.mouth_end - 30.0).max(0.0);
    while s < lines.mouth_end {
        let (point, along) = lines.gaviota.at(s);
        let at = point + left(along) * entry;
        if zones(at + left(along) * 1.5, lines).mouth > 0.9 && lines.lot(at).0 < -0.3 {
            span.push(at);
        }
        s += 0.25;
    }
    let n = span.len();
    (n > 0).then(|| span[n / 2])
}

/// The greenbelt path's middle line: in from Neblina's sidewalk through the mouth, then
/// straight along the foot of Gaviota's north row's back walls, [`PATH_OFFSET`] off them,
/// and on past the row's last lot toward the hillside until it meets a lot or the
/// arena's edge; empty where the greenbelt has no such row.
fn path_line(lines: &Lines) -> Vec<DVec2> {
    let row = gaviota_north_row(lines);
    let reach = PATH_OFFSET + PATH_WIDTH / 2.0 + YARD_WALL_THICKNESS / 2.0;
    let mut chain: Vec<DVec2> = Vec::new();
    for lot in row {
        let mut corners = [lot.sides[0].rear, lot.sides[1].rear];
        corners.sort_by(|a, b| lines.gaviota.along(*a).total_cmp(&lines.gaviota.along(*b)));
        for corner in corners {
            let at = corner - lot.front * reach;
            match chain.last_mut() {
                // A shared corner: once, between what each lot's frame puts there.
                Some(last) if last.distance(at) < 1.0 => *last = (*last + at) / 2.0,
                _ => chain.push(at),
            }
        }
    }
    if chain.len() < 2 {
        return Vec::new();
    }
    let mut line: Vec<DVec2> = Vec::new();
    if let Some(entry) = mouth_entry(lines) {
        let steps = (entry.distance(chain[0]) / 1.0).ceil().max(1.0) as usize;
        line.extend((0..steps).map(|k| entry.lerp(chain[0], k as f64 / steps as f64)));
    }
    line.extend(chain.iter().copied());
    let n = chain.len();
    let east = (chain[n - 1] - chain[n - 2]).normalize();
    let mut p = chain[n - 1] + east;
    while p.x.abs().max(p.y.abs()) < ARENA - 1.0 && lines.lot(p).0 < -PATH_WIDTH {
        line.push(p);
        p += east;
    }
    line
}

/// The greenbelt's path: a narrow concrete walk.
fn greenbelt_path(group: &mut Node, lines: &Lines) {
    let line = path_line(lines);
    if line.len() < 2 {
        return;
    }
    let half = PATH_WIDTH / 2.0;
    let off_ground = |p: DVec2| is_senda_paved(p.x, p.y) || lines.lot(p).0 > -0.1;
    group.children.push(flat(
        ribbon(&line, -half, half, 0.035, &off_ground),
        SIDEWALK_CONCRETE,
    ));
}

/// A smooth step from 0 at `x` 0 to 1 at 1.
fn rise(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the greenbelt's thicket leaves open: its walk, its creek where it shows, and the
/// spawn pads and pickups, each with its room.
struct Thicket {
    path: Line,
    creek: Line,
    clear: Vec<(DVec2, f64)>,
}

impl Thicket {
    fn new(lines: &Lines) -> Self {
        let path = Line(path_line(lines));
        let mut clear: Vec<(DVec2, f64)> = [Team::Blue, Team::Red]
            .into_iter()
            .flat_map(|team| spawn_positions(team, 1.0))
            .map(|spawn| (DVec2::new(spawn.x, spawn.z), SHRUB_PAD_ROOM))
            .collect();
        clear.extend(
            SENDA_PICKUPS
                .iter()
                .map(|pickup| (DVec2::new(pickup.x, pickup.z), SHRUB_PICKUP_ROOM)),
        );
        let creek = Line(creek_line(lines, &path));
        Thicket { path, creek, clear }
    }
}

/// How much of the greenbelt's thicket stands at `p`, from 0 to 1, rising from the lots'
/// walls, the walk, the creek, the spots it keeps clear and the end of the greenbelt
/// short of the hillside, and none in its planted mouth; and how far into the lane's
/// middle it lies, from 0 by the walls to 1, where the thicket stands tallest.
fn mat_weight(p: DVec2, lines: &Lines, thicket: &Thicket) -> (f64, f64) {
    let zone = zones(p, lines);
    let off_walls = -lines.lot(p).0;
    let mut w = (zone.greenbelt - zone.mouth).max(0.0)
        * rise((off_walls - MAT_WALL_ROOM) / MAT_RISE)
        * rise((HILLSIDE_FROM + 4.0 - p.x) / 6.0);
    if thicket.path.0.len() > 1 {
        w *= rise((thicket.path.distance(p) - PATH_WIDTH / 2.0 - MAT_PATH_ROOM) / MAT_RISE);
    }
    if thicket.creek.0.len() > 1 {
        let s = thicket.creek.along(p);
        let wave = 0.6 * (s * 0.23 + 0.4).sin() + 0.4 * (s * 0.61 + 2.1).sin();
        let shows = rise((wave - CREEK_SHOWS) / 0.25);
        let open = rise((thicket.creek.distance(p) - CREEK / 2.0) / MAT_RISE);
        w *= 1.0 - shows * (1.0 - open);
    }
    for &(spot, room) in &thicket.clear {
        w *= rise((spot.distance(p) - room) / MAT_RISE);
    }
    // From Gaviota's side (0) up to Rumbo's (1), measured between the houses' backs.
    let rows = house_front() + HOUSE_DEPTH;
    let up = (lines.gaviota.signed_distance(p) - rows).max(0.0);
    let down = (-lines.rumbo.signed_distance(p) - rows).max(0.0);
    (w, rise((up / (up + down).max(0.1) - 0.15) / 0.7))
}

/// The greenbelt creek's middle line: meandering down the lane between the back walls,
/// where the thicket grows, kept off the walls and the walk; empty where there is no
/// room.
fn creek_line(lines: &Lines, path: &Line) -> Vec<DVec2> {
    let lane = Line(greenbelt_lane());
    if lane.0.len() < 2 {
        return Vec::new();
    }
    let (sway, slow, fast) = CREEK_MEANDER;
    let mut creek = Vec::new();
    let mut s = 0.0;
    while s < lane.length() {
        let (middle, along) = lane.at(s);
        let across = left(along);
        let zone = zones(middle, lines);
        if zone.greenbelt - zone.mouth > 0.9 && middle.x < HILLSIDE_FROM - 2.0 {
            let wanted = sway * (0.7 * (s * slow).sin() + 0.3 * (s * fast + 1.1).sin());
            // Pulled back toward the lane's middle until it clears the walls and the walk.
            let fits = |offset: f64| {
                let at = middle + across * offset;
                -lines.lot(at).0 > CREEK / 2.0 + MAT_WALL_ROOM + 0.4
                    && (path.0.len() < 2
                        || path.distance(at) > PATH_WIDTH / 2.0 + CREEK / 2.0 + 0.3)
            };
            let offset = (0..=6)
                .map(|k| wanted * (1.0 - f64::from(k) / 6.0))
                .find(|&offset| fits(offset));
            if let Some(offset) = offset {
                creek.push(middle + across * offset);
            }
        }
        s += CREEK_STEP;
    }
    creek
}

/// The creek's bed: one narrow continuous runnel down its line, sunk under the thicket.
fn creek_bed(group: &mut Node, creek: &Line) {
    if creek.0.len() < 2 {
        return;
    }
    let half = CREEK / 2.0;
    group.children.push(flat(
        ribbon(&creek.0, -half, half, 0.03, &|_| false),
        CREEK_BED,
    ));
}

/// The thicket's full height at `p` (m), `slope` of the way up the greenbelt from
/// Gaviota's side to Rumbo's.
fn mat_height(p: DVec2, slope: f64) -> f64 {
    let (low, rise) = MAT_SLOPE;
    mat_top(p) * (low + (1.0 - low) * slope) + rise * slope
}

/// The scrub mat's full height at `p` (m): low mounds a few metres across, rippled finer.
fn mat_top(p: DVec2) -> f64 {
    let mound = 0.35 * (0.5 + 0.5 * (p.x * 0.9 + p.y * 1.3).sin() * (p.y * 0.7 - p.x * 1.1).cos())
        + 0.65 * domes(p, MAT_DOME);
    let ripple = MAT_RIPPLE
        * (0.35 * ((p.x * 5.1 + p.y * 2.3).sin() + (p.y * 4.7 - p.x * 1.9 + 0.7).cos())
            + 0.3 * (hash(p.x * 1.9 + p.y * 5.7) - 0.5));
    MAT_HEIGHT.0 + (MAT_HEIGHT.1 - MAT_HEIGHT.0) * mound + ripple
}

/// The thicket's shrubs as domes packed over the ground, from 0 between them to 1 on a
/// dome's crown: each `cell` metres square holds one, jittered off its middle, its radius
/// a little over half the cell, so neighbours touch and overlap and their crevices shade
/// dark.
fn domes(p: DVec2, cell: f64) -> f64 {
    let home = (p / cell).floor();
    let mut crown: f64 = 0.0;
    for dx in -1..=1 {
        for dz in -1..=1 {
            let k = home + DVec2::new(f64::from(dx), f64::from(dz));
            let seed = k.x * 7.13 + k.y * 3.71;
            let middle =
                (k + DVec2::new(0.2 + 0.6 * hash(seed), 0.2 + 0.6 * hash(seed + 0.5))) * cell;
            let radius = cell * (0.6 + 0.3 * hash(seed + 0.9));
            let reach = p.distance(middle) / radius;
            if reach < 1.0 {
                crown = crown.max((1.0 - reach * reach).sqrt() * (0.75 + 0.25 * hash(seed + 1.3)));
            }
        }
    }
    crown
}

/// The greenbelt: its riparian thicket, the willow and mulefat standing out of it, the
/// creek's runnel meandering down its middle, and the view fence across its
/// mouth.
fn greenbelt_scrub() -> Node {
    let mut group = Node::group("senda-greenbelt");
    let lines = Lines::new();
    let thicket = Thicket::new(&lines);
    if let Some(mat) = greenbelt_mat(&lines, &thicket) {
        group.children.push(mat);
    }
    creek_bed(&mut group, &thicket.creek);
    let bush = Arc::new(heart_geometry());
    let lane = greenbelt_lane();
    if lane.len() > 1 {
        let (low, high) = lane.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(low, high), &p| {
                (
                    low.min(p - DVec2::splat(16.0)),
                    high.max(p + DVec2::splat(16.0)),
                )
            },
        );
        let mut z = low.y;
        while z < high.y {
            let mut x = low.x;
            while x < high.x {
                let seed = x * 3.11 + z * 5.03;
                let at = DVec2::new(
                    x + (hash(seed) - 0.5) * MAT_BUSH_SPACING * 0.8,
                    z + (hash(seed + 0.3) - 0.5) * MAT_BUSH_SPACING * 0.8,
                );
                let (w, middle) = mat_weight(at, &lines, &thicket);
                if w > 0.85 && hash(seed + 0.8) < 0.9 {
                    let (least, most) = MAT_BUSH;
                    let size = least + (most - least) * hash(seed + 1.1) * (0.5 + 0.5 * middle);
                    let base = w * mat_height(at, middle) - size * 0.55;
                    let count = MAT_BUSH_LEAVES.len();
                    let tone = MAT_BUSH_LEAVES[(hash(seed + 2.2) * count as f64) as usize % count];
                    leafy(&mut group, &bush, (at, base), (size, 0.85), tone, seed);
                }
                x += MAT_BUSH_SPACING;
            }
            z += MAT_BUSH_SPACING;
        }
    }
    creek_trees(&mut group, &lines, &thicket);
    mouth_planting(&mut group, &lines, &thicket);
    view_fence(&mut group, &lines, &thicket.path);
    batch_in_cells(&mut group, 40.0);
    group
}

/// The greenbelt's mouth on Neblina as the tract planted it, scenery a tank pushes
/// through: a slope of blue-grey groundcover under drifts of tall ornamental grass (a tuft
/// on a staggered grid this fine, m, where a slow noise passes this level) and a few
/// mounded shrubs, up to the view fence this far (m) back from the sidewalk; and the
/// neighbourhood's cluster mailboxes at the back of the sidewalk, cut into the slope
/// behind a low brick retaining wall (its height, thickness and paint), at the first of
/// these shares of the mouth's span along Neblina clear of the walk and the pickups.
const MOUTH_TUFT_STEP: f64 = 0.75;
const MOUTH_DRIFT: f64 = 0.3;
const MOUTH_FENCE_BACK: f64 = 4.2;
const MOUTH_RETAINING: (f64, f64, u32) = (0.55, 0.2, 0x8f5f48);
const MOUTH_MAILBOXES: [f64; 4] = [0.2, 0.8, 0.32, 0.68];
const MOUTH_SHRUBS: [u32; 3] = [0x56603f, 0x5f6a45, 0x4e5a3c];

fn mouth_planting(group: &mut Node, lines: &Lines, thicket: &Thicket) {
    // The mouth's span along Neblina, at the back of its sidewalk.
    let entry = STREET_HALF_WALK + 0.2;
    let mut span: Vec<(DVec2, DVec2)> = Vec::new();
    let mut s = (lines.mouth_end - 30.0).max(0.0);
    while s < lines.mouth_end {
        let (point, along) = lines.gaviota.at(s);
        let at = point + left(along) * entry;
        if zones(at + left(along) * 1.5, lines).mouth > 0.9 && lines.lot(at).0 < -0.3 {
            span.push((at, along));
        }
        s += 0.25;
    }
    if span.len() < 8 {
        return;
    }
    let pickups: Vec<DVec2> = SENDA_PICKUPS
        .iter()
        .map(|pickup| DVec2::new(pickup.x, pickup.z))
        .collect();
    // The mailboxes, on whichever side of the walk is clear of the pickups.
    let mut boxes: Option<(DVec2, DVec2)> = None;
    for share in MOUTH_MAILBOXES {
        let (at, along) = span[((span.len() - 1) as f64 * share) as usize];
        let out = left(along);
        let middle = at + out * 0.75;
        if pickups
            .iter()
            .all(|pickup| pickup.distance(middle) > SHRUB_PICKUP_ROOM + 1.0)
            && (thicket.path.0.len() < 2 || thicket.path.distance(middle) > 1.8)
        {
            boxes = Some((middle, out));
            break;
        }
    }
    if let Some((middle, out)) = boxes {
        let (wall, thick, paint) = MOUTH_RETAINING;
        let span = MAILBOX.0 * MAILBOX_UNITS as f64 + 0.6;
        let depth = MAILBOX.2 + 0.7;
        let yaw = out.x.atan2(out.y);
        let across = -left(out);
        // The back wall and the two side walls, stepping down toward the sidewalk.
        put(
            group,
            rotated(
                box_part(span + thick, wall, thick, paint, 0.0),
                0.0,
                yaw,
                0.0,
            ),
            middle.x + out.x * (depth / 2.0),
            wall / 2.0,
            middle.y + out.y * (depth / 2.0),
        );
        for side in [-1.0, 1.0] {
            for (reach, tall) in [(0.0, wall), (depth / 2.0, wall * 0.6)] {
                let length = depth / 2.0;
                let at = middle
                    + across * (side * span / 2.0)
                    + out * (depth / 2.0 - length / 2.0 - reach);
                put(
                    group,
                    rotated(box_part(thick, tall, length, paint, 0.0), 0.0, yaw, 0.0),
                    at.x,
                    tall / 2.0,
                    at.y,
                );
            }
        }
        mailbox_bank(group, middle, -out);
    }
    // The grass drifts and the shrubs up the slope to the fence and on to the thicket.
    let heart = Arc::new(heart_geometry());
    let (low, high) = span.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(low, high), &(p, _)| {
            (
                low.min(p - DVec2::splat(14.0)),
                high.max(p + DVec2::splat(14.0)),
            )
        },
    );
    let neblina_walk = STREET_HALF_WALK + 0.3;
    let mut row = 0;
    let mut z = low.y;
    while z < high.y {
        let mut x = low.x
            + if row % 2 == 0 {
                0.0
            } else {
                MOUTH_TUFT_STEP / 2.0
            };
        while x < high.x {
            let seed = x * 3.7 + z * 1.9;
            let at = DVec2::new(
                x + (hash(seed) - 0.5) * MOUTH_TUFT_STEP * 0.5,
                z + (hash(seed + 0.3) - 0.5) * MOUTH_TUFT_STEP * 0.5,
            );
            x += MOUTH_TUFT_STEP;
            let zone = zones(at, lines);
            let fence = lines.gaviota.signed_distance(at) - (STREET_HALF_WALK + MOUTH_FENCE_BACK);
            let clear = zone.mouth > 0.7
                && lines.gaviota.signed_distance(at) > neblina_walk
                && lines.lot(at).0 < -0.45
                && fence.abs() > 0.35
                && (thicket.path.0.len() < 2
                    || thicket.path.distance(at) > PATH_WIDTH / 2.0 + 0.35)
                && pickups
                    .iter()
                    .all(|pickup| pickup.distance(at) > SHRUB_PICKUP_ROOM)
                && boxes.is_none_or(|(middle, _)| middle.distance(at) > 1.6)
                && !is_senda_paved(at.x, at.y);
            if !clear {
                continue;
            }
            // The groundcover's first metre at the sidewalk is left low, as the real one is.
            let drift = 0.5
                + 0.3 * (at.x * 0.53 + at.y * 0.31).sin() * (at.y * 0.47 - at.x * 0.29).cos()
                + 0.25 * (hash(seed + 0.7) - 0.5);
            let near_walk = lines.gaviota.signed_distance(at) - neblina_walk;
            if near_walk < 1.2 {
                continue;
            }
            if drift > MOUTH_DRIFT {
                tall_grass(group, at, seed);
            } else if hash(seed + 1.1) < 0.18 {
                let tone = MOUTH_SHRUBS[(hash(seed + 1.7) * 3.0) as usize % 3];
                leafy(
                    group,
                    &heart,
                    (at, -0.05),
                    (0.45 + 0.2 * hash(seed + 2.1), 0.7),
                    tone,
                    seed,
                );
            }
        }
        z += MOUTH_TUFT_STEP * 0.87;
        row += 1;
    }
}

/// The small sycamores and willows along the greenbelt's creek, their open crowns
/// standing out of the thicket: one every [`CREEK_TREE_STEP`] along it, where the thicket
/// is full, clear of the spawn pads, the pickups and the greenbelt's cover trees.
fn creek_trees(group: &mut Node, lines: &Lines, thicket: &Thicket) {
    if thicket.creek.0.len() < 2 {
        return;
    }
    let oaks: Vec<DVec2> = (senda_layout::SENDA_MAP.layout)()
        .into_iter()
        .filter(|cover| cover.kind == CoverKind::Tree)
        .map(|cover| DVec2::new(cover.x, cover.z))
        .collect();
    let mut s = CREEK_TREE_STEP / 2.0;
    let mut k = 0usize;
    while s < thicket.creek.length() {
        let (point, along) = thicket.creek.at(s);
        s += CREEK_TREE_STEP;
        k += 1;
        let side = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        let wanted = point + left(along) * side * 0.9;
        let (x, z) = family_site(wanted, PALE_BROADLEAF);
        let at = DVec2::new(x, z);
        let clear = mat_weight(at, lines, thicket).0 > 0.8
            && oaks.iter().all(|oak| oak.distance(at) > CREEK_TREE_ROOM)
            && thicket
                .clear
                .iter()
                .all(|&(spot, room)| spot.distance(at) > room + 1.0);
        if !clear {
            continue;
        }
        let seed = s * 1.7;
        let (least, most) = CREEK_TREE_CROWN;
        let crown = least + (most - least) * hash(seed);
        let (low, high) = CREEK_TREE_HEIGHT;
        let shape = TreeShape {
            x,
            z,
            w: crown,
            d: crown,
            h: low + (high - low) * hash(seed + 0.6),
        };
        put(group, tree_model(&shape, TreeDetail::Background), x, 0.0, z);
    }
}

/// The spot nearest east of `at` (within 4 m) where [`tree_proportions`] grows a tree of
/// `family`, so a scenery tree's kind is chosen rather than left to its spot's seed.
fn family_site(at: DVec2, family: u32) -> (f64, f64) {
    (0..400)
        .map(|step| (at.x + f64::from(step) * 0.01, at.y))
        .find(|&(x, z)| tree_proportions(x, z, 1.0, 1.0, 1.0).family == family)
        .unwrap_or((at.x, at.y))
}

/// The greenbelt's thicket: one surface over the ground between the back walls, mounded
/// over a few metres, rippled, sloping up from Gaviota's north row toward Rumbo's south
/// row, in dark winter greens and darker in its hollows, rising from the walls, the walk,
/// the creek where it shows, the pickups and the spawn pads, and stopping short of the
/// mouth, where the slope is planted, and of the hillside past the cul-de-sacs.
fn greenbelt_mat(lines: &Lines, thicket: &Thicket) -> Option<Node> {
    let lane = greenbelt_lane();
    if lane.len() < 2 {
        return None;
    }
    let margin = DVec2::splat(16.0);
    let bounds = lane.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(low, high), &p| (low.min(p - margin), high.max(p + margin)),
    );
    let tall = MAT_HEIGHT.1 + MAT_SLOPE.1;
    scrub_surface(
        bounds,
        (MAT_STEP, 4.0, GroundKind::PackedDirt),
        &|p| {
            let (w, slope) = mat_weight(p, lines, thicket);
            (w, w * mat_height(p, slope))
        },
        &|p, height| mat_tone(p, height / tall, &MAT_TONES, MAT_SHADE),
    )
}

/// A thicket's colour at `p`, `light` of the way up from its hollows to its tops: its
/// three `tones` in drifts a metre or two across, each bush a little different, darkening
/// toward `shade` down in the hollows.
fn mat_tone(p: DVec2, light: f64, tones: &[[f32; 3]; 3], shade: [f32; 3]) -> [f32; 3] {
    let drift = (0.5
        + 0.4 * (p.x * 1.7 + p.y * 0.9).sin() * (p.y * 1.3 - p.x * 0.6).cos()
        + 0.15 * (hash(p.x * 3.1 + p.y * 7.3) - 0.5))
        .clamp(0.0, 0.999)
        * 2.0;
    let (a, b) = (tones[drift as usize], tones[drift as usize + 1]);
    let t = drift.fract();
    let light = light.clamp(0.0, 1.0);
    std::array::from_fn(|i| {
        let leaf = a[i] + (b[i] - a[i]) * t as f32;
        shade[i] + (leaf - shade[i]) * (0.35 + 0.65 * light) as f32
    })
}

/// A planted mat as one surface over a grid `step` metres fine across `bounds`: `field`
/// gives each vertex its weight (0 where the mat is not, so cells wholly without it are
/// left out) and its height (m), `tone` its colour from its spot and height, over the
/// dry-grass tile at `grain` times its density, the leaves' fine grain. It shades itself: its
/// mounds are low and close.
fn scrub_surface(
    (low, high): (DVec2, DVec2),
    (step, grain, ground): (f64, f32, GroundKind),
    field: &dyn Fn(DVec2) -> (f64, f64),
    tone: &dyn Fn(DVec2, f64) -> [f32; 3],
) -> Option<Node> {
    let (columns, rows) = (
        ((high.x - low.x) / step).ceil() as usize + 1,
        ((high.y - low.y) / step).ceil() as usize + 1,
    );
    let spot = |k: usize| low + DVec2::new((k % columns) as f64, (k / columns) as f64) * step;
    let fields: Vec<(f64, f64)> = (0..rows * columns).map(|k| field(spot(k))).collect();
    let mut index = vec![u32::MAX; rows * columns];
    let mut positions: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for j in 0..rows - 1 {
        for i in 0..columns - 1 {
            let corners = [
                j * columns + i,
                j * columns + i + 1,
                (j + 1) * columns + i + 1,
                (j + 1) * columns + i,
            ];
            if corners.iter().all(|&k| fields[k].0 <= 0.0) {
                continue;
            }
            let [a, b, c, d] = corners.map(|k| {
                if index[k] == u32::MAX {
                    let p = spot(k);
                    let height = fields[k].1;
                    positions.extend([p.x, height - 0.08, p.y]);
                    colors.push(tone(p, height));
                    index[k] = (positions.len() / 3 - 1) as u32;
                }
                index[k]
            });
            // Wound to face up (+z south, so x then z turns clockwise seen from above).
            indices.extend([a, d, c, a, c, b]);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let count = positions.len() / 3;
    let mut mesh = Mesh::from_f64(
        &positions,
        &vec![0.0; count * 3],
        &vec![0.0; count * 2],
        Some(indices),
    );
    mesh.compute_vertex_normals();
    ground_uvs(&mut mesh, 0.0, 0.0);
    for uv in &mut mesh.uvs {
        *uv = [uv[0] * grain, uv[1] * grain];
    }
    mesh.colors = colors;
    let mut surface = ground_material(ground);
    surface.vertex_colors = true;
    surface.color = crate::scene::Color(0xffffff);
    let mut node = shadowed(Arc::new(mesh), Arc::new(surface));
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = false;
    }
    Some(node)
}

/// What the planted slopes behind Neblina's west row and Gaviota's south row keep clear
/// of: the level's wall, the spawn pads, the pickups, the streets' paving and walks, the
/// bend park, and the lane along the south wall.
struct Outer {
    wall: Line,
    boundary: Vec<DVec2>,
    pads: Vec<DVec2>,
    pickups: Vec<DVec2>,
    streets: Vec<(f64, Line)>,
    park: Option<Park>,
}

impl Outer {
    fn new(lines: &Lines) -> Self {
        let boundary = senda_boundary();
        Outer {
            wall: Line(boundary.iter().chain(boundary.first()).copied().collect()),
            boundary,
            pads: [Team::Blue, Team::Red]
                .into_iter()
                .flat_map(|team| spawn_positions(team, 1.0))
                .map(|spawn| DVec2::new(spawn.x, spawn.z))
                .collect(),
            pickups: SENDA_PICKUPS
                .iter()
                .map(|pickup| DVec2::new(pickup.x, pickup.z))
                .collect(),
            streets: Street::ALL
                .into_iter()
                .map(|street| {
                    let reach = if street == Street::SendaParkway {
                        street.half() + WAY_PARKWAY + WAY_PATH
                    } else {
                        street.half() + PARKWAY + SIDEWALK
                    };
                    (reach, street.line())
                })
                .collect(),
            park: bend_park(lines),
        }
    }

    /// How far inside the level's wall `p` lies (negative outside).
    fn inside(&self, p: DVec2) -> f64 {
        let reach = self.wall.distance(p);
        if inside_boundary(&self.boundary, p) {
            reach
        } else {
            -reach
        }
    }

    /// How much of the outer slopes' mat stands at `p`, from 0 to 1, and how much of that
    /// is the south slope's.
    fn weight(&self, p: DVec2, lines: &Lines) -> (f64, f64) {
        let zone = zones(p, lines);
        let outer = (zone.west + zone.south).clamp(0.0, 1.0) * (1.0 - zone.hillside);
        if outer <= 0.0 || is_senda_paved(p.x, p.y) {
            return (0.0, 0.0);
        }
        if self.park.as_ref().is_some_and(|park| park.contains(p, 0.6)) {
            return (0.0, 0.0);
        }
        let south = (zone.south / outer).clamp(0.0, 1.0);
        let mut w = outer
            * rise((-lines.lot(p).0 - WALL_FOOT - 0.2) / MAT_RISE)
            * rise((self.inside(p) - 0.4) / MAT_RISE);
        for (reach, line) in &self.streets {
            w *= rise((line.distance(p) - reach - 0.3) / MAT_RISE);
        }
        for &pad in &self.pads {
            w *= rise((pad.distance(p) - OUTER_PAD_ROOM) / MAT_RISE);
        }
        for &pickup in &self.pickups {
            w *= rise((pickup.distance(p) - OUTER_PICKUP_ROOM) / MAT_RISE);
        }
        // The south slope's scrub stops short of the lane along the level's wall, which
        // follows the row's back walls rather than one line of z.
        let lane = rise((self.inside(p) - SOUTH_LANE) / 1.0);
        w *= 1.0 - south * (1.0 - lane);
        // Past the west band, groundcover only in drifts over the mulch.
        let band = west_band(p, lines);
        let drift = 0.5
            + 0.3 * (p.x * 0.41 + p.y * 0.23).sin() * (p.y * 0.37 - p.x * 0.19).cos()
            + 0.2 * (hash(p.x * 0.7 + p.y * 1.3) - 0.5);
        let patch = rise((drift - WEST_COVER_DRIFT) / 0.3);
        w *= south + (1.0 - south) * (band + (1.0 - band) * patch);
        (w, south)
    }

    /// The mat's height at `p` (m): `south` of the way from the west slope's low mounds,
    /// rising toward the back walls, to the south slope's taller scrub.
    fn height(p: DVec2, lines: &Lines, south: f64) -> f64 {
        let broad = 0.5 + 0.5 * (p.x * 0.9 + p.y * 1.3).sin() * (p.y * 0.7 - p.x * 1.1).cos();
        let dome = domes(p, WEST_DOME + (SOUTH_DOME - WEST_DOME) * south);
        let mound = 0.3 * broad + 0.7 * dome;
        let ripple = MAT_RIPPLE
            * (0.35 * ((p.x * 5.1 + p.y * 2.3).sin() + (p.y * 4.7 - p.x * 1.9 + 0.7).cos())
                + 0.3 * (hash(p.x * 1.9 + p.y * 5.7) - 0.5));
        let near_walls = rise(1.0 - (-lines.lot(p).0 - WALL_FOOT) / 8.0);
        let band = WEST_MAT.0 + (WEST_MAT.1 - WEST_MAT.0) * mound + WEST_MAT_RISE * near_walls;
        let cover = WEST_COVER.0 + (WEST_COVER.1 - WEST_COVER.0) * mound;
        let west = cover + (band - cover) * west_band(p, lines);
        let scrub = SOUTH_MAT.0 + (SOUTH_MAT.1 - SOUTH_MAT.0) * mound;
        west + (scrub - west) * south + ripple
    }
}

/// The bank under Senda Parkway as the tract planted it, in two bands and a hedge: from the
/// Way's path down past the terrace walk to this share of the way to the back walls, one
/// low carpet of grey-green groundcover (myoporum and prostrate rosemary) over a grid this
/// fine (m), its mounds this high at least and at most (m), each about this wide (m),
/// rising over this many metres from the walks, the crest basins and the Way's path;
/// below it bark mulch with contour rows of round shrubs; and along the foot, this far
/// (m) out from Rumbo's north row's rear fences, a clipped hedge of shrubs this far
/// apart (m), each this big (radius and how tall for its breadth), in dark grey-greens.
const BANK_COVER_DOWN: f64 = 0.46;
const BANK_MAT_STEP: f64 = 0.45;
const BANK_MAT: (f64, f64) = (0.1, 0.34);
const BANK_MAT_DOME: f64 = 0.7;
const BANK_MAT_EDGE: f64 = 0.35;
/// The carpet's tints over the packed-dirt tile, linear (its orange tan drawn to a
/// grey green): three tones in drifts, darker toward this in its hollows.
const BANK_MAT_TONES: [[f32; 3]; 3] = [[0.27, 0.6, 1.9], [0.23, 0.52, 1.6], [0.31, 0.68, 2.2]];
const BANK_MAT_SHADE: [f32; 3] = [0.11, 0.24, 0.62];
const BANK_HEDGE_OUT: f64 = 1.0;
const BANK_HEDGE_STEP: f64 = 0.7;
const BANK_HEDGE: (f64, f64) = (0.5, 0.9);
const BANK_HEDGE_LEAVES: [u32; 3] = [0x4f5a3b, 0x56603f, 0x4a5538];

/// The bank under Senda Parkway: its carpet of groundcover over the upper band and the
/// clipped hedge along its foot, scenery a tank pushes through; the contour rows of
/// shrubs on its mulch below are with the other shrubs.
fn bank_planting() -> Node {
    let mut group = Node::group("senda-bank");
    let lines = Lines::new();
    let outer = Outer::new(&lines);
    let (terrace, spurs) = bank_trails(&lines);
    let walks: Vec<(Line, f64)> = spurs
        .into_iter()
        .map(|line| (Line(line), BANK_SPUR))
        .chain((terrace.len() > 1).then_some((Line(terrace), BANK_WALK)))
        .collect();
    let basins = crest_basin_sites(&lines);
    let top = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH;
    let field = |p: DVec2| {
        let zone = zones(p, &lines);
        if zone.bank_land <= 0.0 || zone.hillside >= 1.0 {
            return (0.0, 0.0);
        }
        let mut w = zone.bank_land
            * (1.0 - zone.hillside)
            * rise((BANK_COVER_DOWN - zone.bank_fall) / 0.05)
            * rise((lines.way.distance(p) - top - 0.1) / BANK_MAT_EDGE)
            * rise((outer.inside(p) + 0.2) / BANK_MAT_EDGE);
        for (line, width) in &walks {
            w *= rise((line.distance(p) - width / 2.0 - 0.1) / BANK_MAT_EDGE);
        }
        for (middle, _) in &basins {
            w *= rise((middle.distance(p) - CREST_BASIN.0 - 0.1) / BANK_MAT_EDGE);
        }
        for &pad in &outer.pads {
            w *= rise((pad.distance(p) - OUTER_PAD_ROOM) / BANK_MAT_EDGE);
        }
        for &pickup in &outer.pickups {
            w *= rise((pickup.distance(p) - OUTER_PICKUP_ROOM) / BANK_MAT_EDGE);
        }
        let mound = 0.7 * domes(p, BANK_MAT_DOME) + 0.3 * domes(p + DVec2::splat(0.31), 0.45);
        let height = BANK_MAT.0 + (BANK_MAT.1 - BANK_MAT.0) * mound;
        (w, w * height)
    };
    let tone =
        |p: DVec2, height: f64| mat_tone(p, height / BANK_MAT.1, &BANK_MAT_TONES, BANK_MAT_SHADE);
    let bounds = (DVec2::new(-ARENA, -ARENA - 2.0), DVec2::new(ARENA, -10.0));
    if let Some(mat) = scrub_surface(
        bounds,
        (BANK_MAT_STEP, 4.0, GroundKind::PackedDirt),
        &field,
        &tone,
    ) {
        group.children.push(mat);
    }
    // The clipped hedge along the bank's foot.
    let heart = Arc::new(heart_geometry());
    for (k, lot) in lines.lots.iter().enumerate() {
        if lot.house.row != Row::RumboNorth {
            continue;
        }
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let mut u = u0;
        let mut n = 0usize;
        while u <= u1 {
            let seed = (k * 53 + n) as f64 * 1.37;
            let at = lot.at(u, lot.rear_v() - BANK_HEDGE_OUT)
                + DVec2::new(hash(seed) - 0.5, hash(seed + 0.3) - 0.5) * 0.12;
            u += BANK_HEDGE_STEP;
            n += 1;
            let clear = outer.inside(at) > BANK_HEDGE.0
                && lines.lot(at).0 < -BANK_HEDGE.0
                && outer
                    .pads
                    .iter()
                    .all(|pad| pad.distance(at) > OUTER_PAD_ROOM)
                && outer
                    .pickups
                    .iter()
                    .all(|pickup| pickup.distance(at) > OUTER_PICKUP_ROOM);
            if !clear {
                continue;
            }
            let (radius, squash) = BANK_HEDGE;
            let tone = BANK_HEDGE_LEAVES[n % BANK_HEDGE_LEAVES.len()];
            leafy(
                &mut group,
                &heart,
                (at, -0.05),
                (radius * (0.92 + 0.16 * hash(seed + 0.7)), squash),
                tone,
                seed,
            );
        }
    }
    batch_in_cells(&mut group, 40.0);
    group
}

/// How much of the dense band behind Neblina's west row's rear walls `p` lies in, from 1
/// within [`WEST_BAND`] of the walls' mulch to 0 a couple of metres past it.
fn west_band(p: DVec2, lines: &Lines) -> f64 {
    rise((WEST_BAND + 1.0 - (-lines.lot(p).0 - WALL_FOOT)) / 2.0)
}

/// The planted slopes round the tract's outer rows, scenery a tank pushes through: behind
/// Neblina's west row the west branch's parkway slope, one dense grey-green mat of
/// rosemary and salvia with clumps of big agaves and a row of olive trees along the foot of
/// the rear walls; behind Gaviota's south row the scrub down toward Gardenia under a near
/// closed band of tree crowns between the cover oaks.
fn outer_slopes() -> Node {
    let mut group = Node::group("senda-outer-slopes");
    let lines = Lines::new();
    let outer = Outer::new(&lines);
    let field = |p: DVec2| {
        let (w, south) = outer.weight(p, &lines);
        (w, w * Outer::height(p, &lines, south))
    };
    let tall = WEST_MAT.1 + WEST_MAT_RISE;
    let tone = |p: DVec2, height: f64| {
        let south = zones(p, &lines).south;
        // Lit against its own tops: the field's low cover against its own height.
        let top = WEST_COVER.1 + (tall - WEST_COVER.1) * west_band(p, &lines).max(south);
        let light = height / top;
        let west = mat_tone(p, light, &WEST_MAT_TONES, WEST_MAT_SHADE);
        let scrub = mat_tone(p, light, &SOUTH_MAT_TONES, SOUTH_MAT_SHADE);
        std::array::from_fn(|i| west[i] + (scrub[i] - west[i]) * south as f32)
    };
    // The west slope's side and the south slope's, meeting along a grid line.
    let edge = ARENA + 1.0;
    let split = DVec2::new(-20.0, 20.0);
    for bounds in [
        (DVec2::new(-edge, -edge), DVec2::new(split.x, edge)),
        (DVec2::new(split.x, split.y), DVec2::new(edge, edge)),
    ] {
        if let Some(mat) = scrub_surface(
            bounds,
            (OUTER_MAT_STEP, 4.0, GroundKind::PackedDirt),
            &field,
            &tone,
        ) {
            group.children.push(mat);
        }
    }
    // Bigger shrubs standing out of the west slope's mat.
    let bush = Arc::new(heart_geometry());
    let mut z = -ARENA;
    while z < ARENA {
        let mut x = -ARENA;
        while x < 0.0 {
            let seed = x * 2.71 + z * 1.37;
            let at = DVec2::new(
                x + (hash(seed) - 0.5) * WEST_SHRUB_SPACING * 0.8,
                z + (hash(seed + 0.3) - 0.5) * WEST_SHRUB_SPACING * 0.8,
            );
            x += WEST_SHRUB_SPACING;
            if hash(seed + 0.6) > 0.7 {
                continue;
            }
            let (w, south) = outer.weight(at, &lines);
            if w < 0.95 || south > 0.1 || west_band(at, &lines) < 0.95 {
                continue;
            }
            let (least, most) = WEST_SHRUB;
            shrub_at(
                &mut group,
                &bush,
                (at, Outer::height(at, &lines, 0.0) - 0.25),
                (least + (most - least) * hash(seed + 0.9), 0.7),
                2,
                &WEST_SHRUB_LEAVES,
                seed,
            );
        }
        z += WEST_SHRUB_SPACING;
    }
    // Clumps of agaves standing out of the west slope's mat.
    let lump = Arc::new(octahedron_geometry(1.0, 1));
    let mut z = -ARENA;
    let mut row = 0;
    while z < ARENA {
        let mut x = -ARENA
            + if row % 2 == 0 {
                0.0
            } else {
                WEST_AGAVE_SPACING / 2.0
            };
        while x < 0.0 {
            let seed = x * 1.93 + z * 3.71;
            let middle = DVec2::new(
                x + (hash(seed) - 0.5) * WEST_AGAVE_SPACING * 0.5,
                z + (hash(seed + 0.2) - 0.5) * WEST_AGAVE_SPACING * 0.5,
            );
            if hash(seed + 0.5) < 0.6 {
                let count = WEST_AGAVES + (hash(seed + 0.9) * 2.99) as usize;
                for n in 0..count {
                    let turn = hash(seed + n as f64 * 1.7) * std::f64::consts::TAU;
                    let reach = if n == 0 {
                        0.0
                    } else {
                        0.75 + 0.45 * hash(seed + n as f64)
                    };
                    let at = middle + DVec2::new(turn.cos(), turn.sin()) * reach;
                    // A tight bed out on the field's mulch, past the band of trees.
                    let zone = zones(at, &lines);
                    let clear = zone.west > 0.95
                        && west_band(at, &lines) < 0.05
                        && outer
                            .pads
                            .iter()
                            .all(|pad| pad.distance(at) > OUTER_PAD_ROOM)
                        && outer
                            .pickups
                            .iter()
                            .all(|pickup| pickup.distance(at) > OUTER_PICKUP_ROOM)
                        && outer.inside(at) > 1.0
                        && !is_senda_paved(at.x, at.y)
                        && outer
                            .park
                            .as_ref()
                            .is_none_or(|park| !park.contains(at, 1.0));
                    if !clear {
                        continue;
                    }
                    let (least, most) = WEST_AGAVE;
                    let size = least + (most - least) * hash(seed + n as f64 * 2.9);
                    let base = -0.05;
                    agave_of(
                        &mut group,
                        &lump,
                        (at, base),
                        (size, turn),
                        WEST_AGAVE_LEAVES,
                    );
                }
            }
            x += WEST_AGAVE_SPACING;
        }
        z += WEST_AGAVE_SPACING * 0.87;
        row += 1;
    }
    let clear = |at: DVec2, crown: f64| {
        outer
            .pads
            .iter()
            .all(|pad| pad.distance(at) > OUTER_PAD_ROOM + crown / 4.0)
            && outer
                .pickups
                .iter()
                .all(|pickup| pickup.distance(at) > OUTER_PICKUP_ROOM + 0.5)
            && outer.inside(at) > crown / 2.0
            && lines.lot(at).0 < -WALL_FOOT
            && !is_senda_paved(at.x, at.y)
    };
    // Behind Neblina's west row, a belt of canopy along the rear walls: two staggered
    // lines, olives near the walls and oaks behind them.
    for (k, lot) in lines.lots.iter().enumerate() {
        if lot.house.row != Row::NeblinaWest {
            continue;
        }
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        for (line, &reach) in WEST_CANOPY_REACH.iter().enumerate() {
            let mut u = u0
                + if line == 0 {
                    WEST_CANOPY_STEP / 2.0
                } else {
                    0.0
                };
            while u < u1 {
                let seed = (k * 17 + line * 5) as f64 + u * 2.3;
                let at = lot.at(u, lot.rear_v() - reach)
                    + DVec2::new(hash(seed) - 0.5, hash(seed + 0.4) - 0.5) * 0.9;
                u += WEST_CANOPY_STEP;
                if line == 0 {
                    let (least, most) = OLIVE_CROWN;
                    let crown = least + (most - least) * hash(seed + 0.9);
                    if clear(at, crown) {
                        let (low, high) = OLIVE_HEIGHT;
                        let height = low + (high - low) * hash(seed + 1.3);
                        olive(&mut group, &bush, at, (crown, height), seed);
                    }
                } else {
                    let (least, most) = CANOPY_CROWN;
                    let crown = least + (most - least) * hash(seed + 1.1) * 0.6;
                    if clear(at, crown) {
                        let (x, z) = oak_site(at.x, at.y);
                        let (low, high) = WEST_TREE_HEIGHT;
                        let shape = TreeShape {
                            x,
                            z,
                            w: crown,
                            d: crown,
                            h: low + (high - low) * hash(seed + 2.3),
                        };
                        put(
                            &mut group,
                            tree_model(&shape, TreeDetail::Background),
                            x,
                            0.0,
                            z,
                        );
                    }
                }
            }
        }
    }
    let oaks: Vec<DVec2> = (senda_layout::SENDA_MAP.layout)()
        .into_iter()
        .filter(|cover| cover.kind == CoverKind::Tree)
        .map(|cover| DVec2::new(cover.x, cover.z))
        .collect();
    // Small trees at the foot of the bank behind Rumbo's north row, where neighbours'
    // lot lines meet the rear fences.
    let mut turn = 0usize;
    for (i, lot) in lines.lots.iter().enumerate() {
        if lot.house.row != Row::RumboNorth {
            continue;
        }
        for side in &lot.sides {
            if side.neighbour.is_none_or(|j| j < i) {
                continue;
            }
            let reach = BANK_TOE_REACH[turn % BANK_TOE_REACH.len()];
            turn += 1;
            let at = side.rear - lot.front * (YARD_WALL_THICKNESS / 2.0 + reach);
            let seed = i as f64 * 6.1 + reach;
            let (least, most) = BANK_TOE_CROWN;
            let crown = least + (most - least) * hash(seed);
            let clear_here = outer
                .pickups
                .iter()
                .all(|pickup| pickup.distance(at) > BANK_TOE_PICKUP_ROOM)
                && oaks.iter().all(|oak| oak.distance(at) > SOUTH_CANOPY_ROOM)
                && clear(at, crown);
            if !clear_here {
                continue;
            }
            let (low, high) = BANK_TOE_HEIGHT;
            // Oaks, never the village's conifers, whatever the spot's seed would grow.
            let (x, z) = oak_site(at.x, at.y);
            let shape = TreeShape {
                x,
                z,
                w: crown,
                d: crown,
                h: low + (high - low) * hash(seed + 0.8),
            };
            put(
                &mut group,
                tree_model(&shape, TreeDetail::Background),
                x,
                0.0,
                z,
            );
        }
    }
    // The band of crowns behind Gaviota's south row, between the cover oaks.
    let (least, most) = CANOPY_CROWN;
    let (low, high) = CANOPY_HEIGHT;
    for (k, lot) in lines.lots.iter().enumerate() {
        if lot.house.row != Row::GaviotaSouth {
            continue;
        }
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        for (line, &reach) in SOUTH_CANOPY_REACH.iter().enumerate() {
            let shift = if line == 0 { 0.25 } else { 0.75 };
            let mut u = u0 + (u1 - u0) * shift - SOUTH_CANOPY_STEP;
            while u < u1 {
                let seed = (k * 13 + line * 7) as f64 + u * 1.7;
                let at = lot.at(u, lot.rear_v() - reach)
                    + DVec2::new(hash(seed) - 0.5, hash(seed + 0.4) - 0.5) * 0.8;
                u += SOUTH_CANOPY_STEP;
                if u - SOUTH_CANOPY_STEP < u0
                    || outer.inside(at) < SOUTH_LANE - 0.5
                    || oaks.iter().any(|oak| oak.distance(at) < SOUTH_CANOPY_ROOM)
                    || !clear(at, 2.0)
                {
                    continue;
                }
                let crown = least + (most - least) * hash(seed + 1.1);
                let (x, z) = oak_site(at.x, at.y);
                let shape = TreeShape {
                    x,
                    z,
                    w: crown,
                    d: crown,
                    h: low + (high - low) * hash(seed + 2.3),
                };
                put(
                    &mut group,
                    tree_model(&shape, TreeDetail::Background),
                    x,
                    0.0,
                    z,
                );
            }
        }
    }
    batch_in_cells(&mut group, 40.0);
    group
}

/// An olive tree at `at`, its crown `crown` across and `height` tall: a short dark trunk
/// leaning one way and forking, under a loose grey-green crown of several lumps.
fn olive(group: &mut Node, bush: &Arc<Mesh>, at: DVec2, (crown, height): (f64, f64), seed: f64) {
    let lean = hash(seed + 2.1) * std::f64::consts::TAU;
    let trunk = height * 0.5;
    for (n, tilt) in [0.18, -0.3].into_iter().enumerate() {
        let turn = lean + n as f64 * 2.4;
        let mut limb = cylinder_part(0.09 - n as f64 * 0.025, trunk, OLIVE_BARK, 6);
        limb.rotation = DQuat::from_rotation_y(turn) * DQuat::from_rotation_z(tilt);
        let top = DVec2::new(turn.cos(), -turn.sin()) * (-tilt).sin() * trunk / 2.0;
        put(group, limb, at.x + top.x, trunk / 2.0, at.y + top.y);
    }
    let lumps = 5;
    for n in 0..lumps {
        let k = seed + n as f64 * 3.1;
        let angle = n as f64 / lumps as f64 * std::f64::consts::TAU + hash(k);
        let reach = if n == 0 { 0.0 } else { crown * 0.28 };
        let spot = at + DVec2::new(angle.cos(), angle.sin()) * reach;
        let r = crown * (0.3 + 0.08 * hash(k + 0.5));
        let tone = OLIVE_LEAVES[(hash(k + 1.1) * 3.0) as usize % 3];
        let y = height - crown * 0.25 + if n == 0 { 0.15 } else { -0.1 * hash(k + 3.3) };
        leafy(group, bush, (spot, y - r * 0.62 * 0.8), (r, 0.62), tone, k);
    }
}

/// The tubular steel view fence across the greenbelt's mouth behind Neblina's
/// sidewalk, on capped block pilasters, open where the path comes through.
fn view_fence(group: &mut Node, lines: &Lines, path: &Line) {
    let entry = STREET_HALF_WALK + MOUTH_FENCE_BACK;
    let mut runs: Vec<Vec<DVec2>> = vec![Vec::new()];
    let mut s = (lines.mouth_end - 30.0).max(0.0);
    while s < lines.mouth_end {
        let (point, along) = lines.gaviota.at(s);
        let at = point + left(along) * entry;
        let open = zones(at + left(along) * 1.5, lines).mouth > 0.9
            && lines.lot(at).0 < -0.3
            && (path.0.len() < 2 || path.distance(at) > 0.9);
        let last = runs.last_mut().expect("a run");
        if open {
            last.push(at);
        } else if !last.is_empty() {
            runs.push(Vec::new());
        }
        s += 0.12;
    }
    let (height, pilaster, spacing) = VIEW_FENCE;
    let (picket, gap) = PICKETS;
    for run in runs.into_iter().filter(|run| run.len() > 4) {
        let line = Line(run);
        let length = line.length();
        // Pilasters at both ends and between, evenly.
        let bays = (length / spacing).ceil().max(1.0) as usize;
        for k in 0..=bays {
            let (at, along) = line.at(length * k as f64 / bays as f64);
            let yaw = along.x.atan2(along.y);
            let tall = height + 0.15;
            put(
                group,
                rotated(
                    box_part(pilaster, tall, pilaster, YARD_WALL_COLOR, 0.0),
                    0.0,
                    yaw,
                    0.0,
                ),
                at.x,
                tall / 2.0,
                at.y,
            );
            put(
                group,
                rotated(
                    box_part(pilaster + 0.08, 0.07, pilaster + 0.08, YARD_WALL_CAP.2, 0.0),
                    0.0,
                    yaw,
                    0.0,
                ),
                at.x,
                tall + 0.035,
                at.y,
            );
        }
        // Top and bottom rails, then the pickets.
        let mut s = 0.0;
        while s < length {
            let (at, along) = line.at(s);
            let yaw = along.x.atan2(along.y);
            if s.rem_euclid(1.2) < gap {
                for y in [0.12, height - 0.05] {
                    put(
                        group,
                        rotated(box_part(0.04, 0.04, 1.2, FENCE_STEEL, 0.0), 0.0, yaw, 0.0),
                        at.x + along.x * 0.6,
                        y,
                        at.y + along.y * 0.6,
                    );
                }
            }
            put(
                group,
                box_part(picket, height, picket, FENCE_STEEL, 0.0),
                at.x,
                height / 2.0,
                at.y,
            );
            s += gap;
        }
    }
}

/// The bank's concrete terrace walk between Senda Parkway's path and Rumbo's back fences
/// with its spur walks up to the path, and the terrace drains, concrete V-ditches with a
/// crease down their middle, following the hillside's contours round the cul-de-sacs' ends,
/// all left out round
/// the spawn pads and where they cross Senda Parkway.
fn trails(group: &mut Node, lines: &Lines) {
    let pads: Vec<DVec2> = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    let way = Street::SendaParkway.line();
    let way_reach = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH;
    // The level's wall stands outside its boundary line: a drain stops short of the
    // wall's face on either side rather than running through it.
    let boundary = senda_boundary();
    let wall = Line(boundary.iter().chain(boundary.first()).copied().collect());
    let at_wall = |p: DVec2| {
        let room = if inside_boundary(&boundary, p) {
            TRAIL_WALL_ROOM
        } else {
            BOUNDARY_THICKNESS + TRAIL_WALL_ROOM
        };
        wall.distance(p) < room
    };
    let off_pads = |p: DVec2| {
        pads.iter().any(|pad| pad.distance(p) < TRAIL_SPAWN_ROOM)
            || way.distance(p) < way_reach
            || at_wall(p)
    };
    let (terrace, spurs) = bank_trails(lines);
    let walks = spurs
        .into_iter()
        .map(|line| (line, BANK_SPUR))
        .chain((terrace.len() > 1).then_some((terrace, BANK_WALK)));
    for (line, width) in walks {
        let half = width / 2.0;
        group.children.push(flat(
            ribbon(&line, -half, half, 0.03, &off_pads),
            SIDEWALK_CONCRETE,
        ));
        let (edge, paler) = BANK_WALK_EDGE;
        for side in [-1.0, 1.0] {
            let (a, b) = (side * half, side * (half - edge));
            group.children.push(flat(
                ribbon(&line, a.min(b), a.max(b), 0.033, &off_pads),
                paler,
            ));
        }
    }
    let mut drains: Vec<(Vec<DVec2>, f64)> = Vec::new();
    drains.extend(
        hillside_trails()
            .into_iter()
            .map(|line| (line, HILL_TRAIL_WIDTH)),
    );
    // Each from its turning circle's far rim onto the hillside, short of the east wall.
    drains.extend(cul_de_sac_ends().into_iter().map(|(start, out)| {
        let to_wall = (ARENA - TRAIL_WALL_ROOM - start.x) / out.x.max(0.1);
        (
            vec![start, start + out * (CUL_DE_SAC_END * 1.6).min(to_wall)],
            TRAIL_WIDTH,
        )
    }));
    for (line, width) in &drains {
        group.children.push(flat(
            ribbon(line, -width / 2.0, width / 2.0, 0.025, &off_pads),
            TRAIL,
        ));
        group.children.push(flat(
            ribbon(
                line,
                -TRAIL_CREASE.0 / 2.0,
                TRAIL_CREASE.0 / 2.0,
                0.028,
                &off_pads,
            ),
            TRAIL_CREASE.1,
        ));
    }
}

/// The hillside trails east of the cul-de-sacs ([`HILLSIDE_TRAIL_REACH`]): for each
/// metre of z, the furthest east a spot lies that is that far from a house's walls or a
/// turning circle's sidewalk, smoothed; where nothing of the neighbourhood is that near,
/// the trail carries on straight north or south.
fn hillside_trails() -> Vec<Vec<DVec2>> {
    // The neighbourhood's east edge as discs: each house's walls (by the half diagonal
    // of its box, the corners rounded off) and each turning circle out to its sidewalk.
    let mut discs: Vec<(DVec2, f64)> = senda_houses()
        .iter()
        .filter(|house| house.x > 0.0)
        .map(|house| {
            (
                DVec2::new(house.x, house.z),
                house.w.hypot(HOUSE_DEPTH) / 2.0,
            )
        })
        .collect();
    discs.extend(
        Street::ALL
            .into_iter()
            .filter_map(Street::cul_de_sac)
            .map(|middle| (middle, CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK)),
    );
    let (from, to) = HILLSIDE_TRAIL_SPAN;
    HILLSIDE_TRAIL_REACH
        .iter()
        .map(|&reach| {
            let edge: Vec<Option<f64>> = (from as i32..=to as i32)
                .map(|z| {
                    let z = f64::from(z);
                    discs
                        .iter()
                        .filter_map(|&(middle, radius)| {
                            let r = radius + reach;
                            let dz = z - middle.y;
                            (dz.abs() < r).then(|| middle.x + (r * r - dz * dz).sqrt())
                        })
                        .reduce(f64::max)
                })
                .collect();
            // Past the neighbourhood's ends, straight on from the nearest row it reaches.
            let first = edge.iter().flatten().next().copied().unwrap_or(ARENA);
            let mut last = first;
            let filled: Vec<f64> = edge
                .iter()
                .map(|x| {
                    last = x.unwrap_or(last);
                    last
                })
                .collect();
            let half = HILLSIDE_TRAIL_SMOOTH as usize;
            let smoothed: Vec<f64> = (0..filled.len())
                .map(|i| {
                    let window = &filled[i.saturating_sub(half)..(i + half + 1).min(filled.len())];
                    window.iter().sum::<f64>() / window.len() as f64
                })
                .collect();
            // The tract's wall stands along the arena's east edge: a trail keeps to the
            // side of it most of the trail lies on, running along the wall rather than
            // through it.
            let wall = ARENA + BOUNDARY_THICKNESS / 2.0;
            let reach = BOUNDARY_THICKNESS / 2.0 + TRAIL_WALL_ROOM;
            let inside = smoothed.iter().filter(|&&x| x < wall).count() * 2 > smoothed.len();
            smoothed
                .into_iter()
                .enumerate()
                .map(|(i, x)| {
                    let x = if inside {
                        x.min(wall - reach)
                    } else {
                        x.max(wall + reach)
                    };
                    DVec2::new(x, from + i as f64)
                })
                .collect()
        })
        .collect()
}

/// The walks on the bank: one terrace walk along it a third of the way down from
/// Senda Parkway's path wherever the bank is wide enough, and the straight spur walks from
/// it up to the Way's path every few lots.
fn bank_trails(lines: &Lines) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
    let (way, rumbo) = (&lines.way, &lines.rumbo);
    let top = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH;
    // The bank's foot and top across from a spot on Rumbo, found by stepping up from
    // the back walls' mulch toward the Way.
    let across = |s: f64| {
        let (point, along) = rumbo.at(s);
        let out = left(along);
        let fence = point + out * (lines.back_walls(rumbo, s, 1.0) + WALL_FOOT + 0.3);
        let mut path = fence;
        while way.distance(path) > top && path.distance(fence) < ARENA {
            path += out * 0.1;
        }
        (fence, path)
    };
    let mut terrace: Vec<DVec2> = Vec::new();
    let mut drains: Vec<Vec<DVec2>> = Vec::new();
    let mut s = 0.0;
    while s < rumbo.length() {
        let (fence, path) = across(s);
        if fence.distance(path) > BANK_TRAIL_ROOM {
            let at = path.lerp(fence, BANK_TERRACE_DOWN);
            if let Some(&last) = terrace.last()
                && BANK_SPURS.iter().any(|&x| (last.x < x) != (at.x < x))
            {
                drains.push(vec![at, path]);
            }
            terrace.push(at);
        }
        s += 2.0;
    }
    (terrace, drains)
}

/// Where each cul-de-sac's end meets the hillside: the back of its sidewalk straight on
/// from its street, and the way on out.
fn cul_de_sac_ends() -> Vec<(DVec2, DVec2)> {
    Street::ALL
        .into_iter()
        .filter_map(|street| {
            let middle = street.cul_de_sac()?;
            let line = street.line();
            let (_, along) = line.at(line.length());
            Some((
                middle + along * (CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK),
                along,
            ))
        })
        .collect()
}

/// A cul-de-sac's turning circle round `middle`, with its kerb ring and the planted
/// island in its middle.
fn cul_de_sac(group: &mut Node, middle: DVec2) {
    let disc = |inner: f64, outer: f64, color: u32, y: f64| {
        let mut geometry = RingGeometry {
            inner_radius: inner,
            outer_radius: outer,
            theta_segments: 28,
            ..RingGeometry::default()
        }
        .build();
        geometry.rotate_x(-FRAC_PI_2);
        let mut node = flat(geometry, color);
        node.position = glam::DVec3::new(middle.x, y, middle.y);
        node
    };
    group
        .children
        .push(disc(0.0, CUL_DE_SAC_RADIUS, ASPHALT, STREET_TOP + 0.002));
    group.children.push(disc(
        CUL_DE_SAC_RADIUS,
        CUL_DE_SAC_RADIUS + KERB_WIDTH,
        KERB,
        KERB_HEIGHT,
    ));
    let walk = CUL_DE_SAC_RADIUS + PARKWAY;
    group
        .children
        .push(disc(walk, walk + SIDEWALK, SIDEWALK_CONCRETE, WALK_TOP));
    group.children.push(disc(
        CUL_DE_SAC_ISLAND,
        CUL_DE_SAC_ISLAND + KERB_WIDTH,
        KERB,
        KERB_HEIGHT,
    ));
    group
        .children
        .push(disc(0.0, CUL_DE_SAC_ISLAND, 0x5f7c3e, 0.1));
}

/// A house's driveway, from its garage door across the front yard, sidewalk and
/// parkway to the kerb, the front walk from its door to the sidewalk, and the patio
/// off its back wall: placed from the openings its plan cuts, whichever way it faces.
fn driveway(group: &mut Node, design: &HouseDesign, house: &SendaHouse) {
    let (x, z, setback) = (house.x, house.z, house.setback);
    let (centre, out, along, _) = wall_frames(design.w, design.d)[design.door_wall];
    let plan = design.wall_plan(design.door_wall);
    let out = design.to_world(out);
    let yaw = out.x.atan2(out.z);
    let mut slab = |u: f64, width: f64, length: f64, color: u32, top: f64| {
        let face = design.to_world(centre + along * u);
        let middle = face + out * (length / 2.0);
        let piece = rotated(box_part(width, SLAB, length, color, 0.0), 0.0, yaw, 0.0);
        put(group, piece, x + middle.x, top - SLAB / 2.0, z + middle.z);
    };
    if let (Some(garage), Some(door)) = (plan.garage(), plan.door()) {
        let width = plan.garage_width() + DRIVEWAY_PAST_DOOR;
        slab(garage, width, setback + SIDEWALK + PARKWAY, DRIVEWAY, 0.055);
        // The paver border on the walk's side, across the front yard.
        let edge = garage + (door - garage).signum() * (width + PAVER_STRIP.0) / 2.0;
        slab(edge, PAVER_STRIP.0, setback, PAVER_STRIP.1, 0.056);
    }
    if let Some(door) = plan.door() {
        slab(door, FRONT_WALK, setback, SIDEWALK_CONCRETE, 0.052);
    }
}

/// A flat quadrilateral through `corners` on the ground at height `y`, its face up
/// whichever way round they run.
fn quad(corners: [DVec2; 4], y: f64) -> Mesh {
    let [a, b, c, _] = corners;
    // Up-facing when the corners turn clockwise seen from above (+z south).
    let turn = (b.y - a.y) * (c.x - a.x) - (b.x - a.x) * (c.y - a.y);
    let order = if turn > 0.0 {
        [0, 1, 2, 0, 2, 3]
    } else {
        [0, 2, 1, 0, 3, 2]
    };
    let mut positions = Vec::with_capacity(18);
    let mut normals = Vec::with_capacity(18);
    let mut uvs = Vec::with_capacity(12);
    for index in order {
        let p = corners[index];
        positions.extend([p.x, y, p.y]);
        normals.extend([0.0, 1.0, 0.0]);
        uvs.extend([p.x / 4.0, p.y / 4.0]);
    }
    Mesh::from_f64(&positions, &normals, &uvs, None)
}

/// The stretches (from, to metres along it) of Senda Parkway's median: all of its line
/// but the opening across Neblina's mouth, found to a few centimetres.
fn median_spans(way: &Line) -> Vec<(f64, f64)> {
    const STEP: f64 = 0.05;
    let length = way.length();
    let mut spans = Vec::new();
    let mut start = None;
    let mut s = 0.0;
    while s <= length {
        let open = in_median_opening(way.at(s).0, 0.0);
        match (open, start) {
            (false, None) => start = Some(s),
            (true, Some(from)) => {
                spans.push((from, s));
                start = None;
            }
            _ => {}
        }
        s += STEP;
    }
    if let Some(from) = start {
        spans.push((from, length));
    }
    spans
}

/// The part of `line` from `from` to `to` metres along it.
fn cut_line(line: &Line, from: f64, to: f64) -> Vec<DVec2> {
    if to <= from {
        return Vec::new();
    }
    let mut points = vec![line.at(from).0];
    let mut along = 0.0;
    for pair in line.0.windows(2) {
        along += pair[0].distance(pair[1]);
        if along > from && along < to {
            points.push(pair[1]);
        }
    }
    points.push(line.at(to).0);
    points
}

/// A round nose's half ring about `centre`, bulging toward `out`, from radius `inner`
/// (0 for its top) to `outer`, flat at height `y`.
fn nose(centre: DVec2, out: DVec2, (inner, outer): (f64, f64), y: f64) -> Mesh {
    const SEGMENTS: usize = 12;
    let side = left(out);
    let point = |k: usize, radius: f64| {
        let angle = std::f64::consts::PI * k as f64 / SEGMENTS as f64;
        centre + (side * angle.cos() + out * angle.sin()) * radius
    };
    let mut positions = Vec::new();
    for k in 0..SEGMENTS {
        let corners = [
            point(k, inner.max(1e-3)),
            point(k, outer),
            point(k + 1, outer),
            point(k + 1, inner.max(1e-3)),
        ];
        let [a, b, c, _] = corners;
        // Up-facing when the corners turn clockwise seen from above (+z south).
        let turn = (b.y - a.y) * (c.x - a.x) - (b.x - a.x) * (c.y - a.y);
        let order = if turn > 0.0 {
            [0, 1, 2, 0, 2, 3]
        } else {
            [0, 2, 1, 0, 3, 2]
        };
        for index in order {
            positions.extend([corners[index].x, y, corners[index].y]);
        }
    }
    let count = positions.len() / 3;
    let normals: Vec<f64> = (0..count).flat_map(|_| [0.0, 1.0, 0.0]).collect();
    let uvs: Vec<f64> = positions
        .chunks(3)
        .flat_map(|p| [p[0] / 4.0, p[2] / 4.0])
        .collect();
    Mesh::from_f64(&positions, &normals, &uvs, None)
}

/// A slab `w` across a lot's front by `d` toward its street, its top at `top`, centred
/// at `(u, v)` in the lot's frame.
fn lot_slab(
    group: &mut Node,
    lot: &Lot,
    (u, v): (f64, f64),
    (w, d): (f64, f64),
    top: f64,
    color: u32,
) {
    let at = lot.at(u, v);
    let yaw = lot.front.x.atan2(lot.front.y);
    let piece = rotated(box_part(w, SLAB, d, color, 0.0), 0.0, yaw, 0.0);
    put(group, piece, at.x, top - SLAB / 2.0, at.y);
}

/// The ground of every walled yard: decomposed granite inside the walls behind the gate
/// returns, the patio against the house's back wall, on every other lot a turf patch
/// behind it, the planting strip along the back wall, and outside the back wall the bark
/// mulch along its foot.
fn yard_grounds(group: &mut Node, lots: &[Lot]) {
    let mut lawn = ground_material(GroundKind::DryGrass);
    lawn.vertex_colors = true;
    lawn.color = crate::scene::Color(0xffffff);
    let lawn = Arc::new(lawn);
    for (k, lot) in lots.iter().enumerate() {
        let [low, high] = lot.sides;
        group.children.push(flat(
            quad([low.gate, high.gate, high.rear, low.rear], 0.03),
            YARD_GRAVEL,
        ));
        let foot = [
            low.rear,
            high.rear,
            high.rear - lot.front * WALL_FOOT,
            low.rear - lot.front * WALL_FOOT,
        ];
        group.children.push(flat(quad(foot, 0.028), BED_MULCH));
        let back = house_back(lot);
        let depth = patio_depth(lot);
        let paving = if k % 3 == 1 {
            PATIO_PAVERS
        } else {
            PATIO_CONCRETE
        };
        lot_slab(
            group,
            lot,
            (0.0, back - depth / 2.0),
            (2.0 * lot.half_width - 0.2, depth),
            0.05,
            paving,
        );
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let planter = lot.rear_v() + 0.1 + YARD_PLANTER / 2.0;
        lot_slab(
            group,
            lot,
            ((u0 + u1) / 2.0, planter),
            (u1 - u0 - 0.2, YARD_PLANTER),
            0.042,
            BED_MULCH,
        );
        // Narrower beds along the side walls, from the back bed to the patio.
        let (from, to) = (lot.rear_v() + 0.1 + YARD_PLANTER, back - depth - 0.1);
        if to - from > 0.4 {
            for edge in [u0 + 0.1 + SIDE_PLANTER / 2.0, u1 - 0.1 - SIDE_PLANTER / 2.0] {
                lot_slab(
                    group,
                    lot,
                    (edge, (from + to) / 2.0),
                    (SIDE_PLANTER, to - from),
                    0.042,
                    BED_MULCH,
                );
            }
        }
        let turf = lot.yard - depth - YARD_PLANTER - 0.2;
        if k.is_multiple_of(2) && turf > 0.5 {
            let side = turf_side(k);
            let width = (u1 - u0 - 2.0 * SIDE_PLANTER) * TURF_SHARE;
            let edge = if side > 0.0 {
                u1 - 0.2 - SIDE_PLANTER
            } else {
                u0 + 0.2 + SIDE_PLANTER
            };
            let (u, v) = (edge - side * width / 2.0, back - depth - 0.1 - turf / 2.0);
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(a, b)| lot.at(u + a * width / 2.0, v + b * turf / 2.0));
            let mut mesh = quad(corners, 0.046);
            ground_uvs(&mut mesh, 0.0, 0.0);
            mesh.colors = vec![TURF; mesh.positions.len()];
            group
                .children
                .push(shadow_receiver(Arc::new(mesh), lawn.clone()));
        }
    }
}

/// The front yards' small lawns: the parkway's strip of clipped lawn between the kerb and
/// the sidewalk across each lot (its driveway apron is laid over it), and a lawn panel in
/// the front yard on the door's side of the front walk, short of the planting bed along
/// the house.
fn front_lawns(group: &mut Node, lots: &[Lot]) {
    let mut grass = ground_material(GroundKind::DryGrass);
    grass.vertex_colors = true;
    grass.color = crate::scene::Color(0xffffff);
    let grass = Arc::new(grass);
    let lot_lawn = |group: &mut Node, lot: &Lot, middle: (f64, f64), size: (f64, f64), top: f64| {
        let mesh = lawn_patch(lot, middle, size, top);
        group
            .children
            .push(shadow_receiver(Arc::new(mesh), grass.clone()));
    };
    for lot in lots {
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let walk = lot.half_depth + lot.house.setback + SIDEWALK;
        let (near, far) = (walk + 0.02, walk + PARKWAY - KERB_WIDTH - 0.02);
        lot_lawn(
            group,
            lot,
            ((u0 + u1) / 2.0, (near + far) / 2.0),
            (u1 - u0, far - near),
            0.045,
        );
        let design = lot.house.design();
        let (centre, _, along, _) = wall_frames(design.w, design.d)[design.door_wall];
        let plan = design.wall_plan(design.door_wall);
        let (Some(door), Some(garage)) = (plan.door(), plan.garage()) else {
            continue;
        };
        let in_lot = |u: f64| {
            let at = design.to_world(centre + along * u);
            lot.local(DVec2::new(lot.house.x + at.x, lot.house.z + at.z))
                .0
        };
        let (door, garage) = (in_lot(door), in_lot(garage));
        let side = (door - garage).signum();
        let from = door + side * (FRONT_WALK / 2.0 + 0.15);
        let to = if side > 0.0 { u1 - 0.25 } else { u0 + 0.25 };
        let (back, front) = (
            lot.half_depth + FRONT_BED,
            lot.half_depth + lot.house.setback - 0.05,
        );
        if (to - from) * side > 0.5 && front - back > 0.5 {
            lot_lawn(
                group,
                lot,
                ((from + to) / 2.0, (back + front) / 2.0),
                ((to - from).abs(), front - back),
                0.04,
            );
        }
    }
}

/// A lawn on `lot` round `(u, v)` of `(w, d)` (m) with its top `top` up: the dry-grass tile
/// watered a muted mid green, mottled from cell to cell (a grid this fine, m) as a real
/// lawn's patches of thicker and thinner grass are.
const LAWN_CELL: f64 = 0.6;

fn lawn_patch(lot: &Lot, (u, v): (f64, f64), (w, d): (f64, f64), top: f64) -> Mesh {
    let (columns, rows) = (
        (w / LAWN_CELL).ceil().max(1.0) as usize,
        (d / LAWN_CELL).ceil().max(1.0) as usize,
    );
    let mut positions: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    for j in 0..=rows {
        for i in 0..=columns {
            let at = lot.at(
                u - w / 2.0 + w * i as f64 / columns as f64,
                v - d / 2.0 + d * j as f64 / rows as f64,
            );
            positions.extend([at.x, top, at.y]);
            let mottle = 0.88
                + 0.12 * (at.x * 0.9 + at.y * 1.3).sin() * (at.y * 0.7 - at.x * 1.1).cos()
                + 0.1 * (hash(at.x * 3.1 + at.y * 7.7) - 0.5);
            colors.push(FRONT_LAWN_TINT.map(|c| c * mottle as f32));
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    let stride = columns + 1;
    for j in 0..rows {
        for i in 0..columns {
            let [a, b, c, e] = [
                j * stride + i,
                j * stride + i + 1,
                (j + 1) * stride + i + 1,
                (j + 1) * stride + i,
            ]
            .map(|k| k as u32);
            // Wound to face up whichever way the lot's frame turns.
            let (pa, pb, pc) = (
                DVec2::new(positions[a as usize * 3], positions[a as usize * 3 + 2]),
                DVec2::new(positions[b as usize * 3], positions[b as usize * 3 + 2]),
                DVec2::new(positions[c as usize * 3], positions[c as usize * 3 + 2]),
            );
            let turn = (pb - pa).perp_dot(pc - pa);
            if turn < 0.0 {
                indices.extend([a, b, c, a, c, e]);
            } else {
                indices.extend([a, c, b, a, e, c]);
            }
        }
    }
    let count = positions.len() / 3;
    let normals: Vec<f64> = (0..count).flat_map(|_| [0.0, 1.0, 0.0]).collect();
    let mut mesh = Mesh::from_f64(&positions, &normals, &vec![0.0; count * 2], Some(indices));
    ground_uvs(&mut mesh, 0.0, 0.0);
    mesh.colors = colors;
    mesh
}

/// The little park on the outside of Neblina's bend, in its own frame: `out` toward the
/// street, `across` along it, from its middle.
struct Park {
    middle: DVec2,
    out: DVec2,
    across: DVec2,
    /// Its corners: at the sidewalk and at its back wall, either side.
    corners: [DVec2; 4],
    /// Half its width across, and how far its sidewalk and back wall lie from its middle.
    half_width: f64,
    half_depth: f64,
}

impl Park {
    fn at(&self, u: f64, v: f64) -> DVec2 {
        self.middle + self.across * u + self.out * v
    }

    /// Whether `p` lies in the park or within `room` (m) of it.
    fn contains(&self, p: DVec2, room: f64) -> bool {
        let offset = p - self.middle;
        offset.dot(self.across).abs() < self.half_width + room
            && offset.dot(self.out).abs() < self.half_depth + room
    }

    /// Its lawn's radius at angle `theta` (0 across, a quarter turn out to the street):
    /// an ellipse with a bite out of its street side, the kidney the walk curves round.
    fn lawn(&self, theta: f64) -> f64 {
        let (ru, rv) = (self.half_width * PARK_LAWN.0, self.half_depth * PARK_LAWN.1);
        let ellipse = 1.0 / ((theta.cos() / ru).powi(2) + (theta.sin() / rv).powi(2)).sqrt();
        let bite = (theta - FRAC_PI_2).rem_euclid(std::f64::consts::TAU);
        let bite = bite.min(std::f64::consts::TAU - bite);
        ellipse * (1.0 - PARK_LAWN.2 * (-bite * bite / 0.35).exp())
    }

    /// The walk's middle line: in from the sidewalk on one side, round the back of the
    /// lawn and out to the sidewalk on the other.
    fn walk(&self) -> Vec<DVec2> {
        let reach = |theta: f64| self.lawn(theta) + PARK_WALK / 2.0 + 0.25;
        let side = reach(0.0);
        let mut line = vec![self.at(-side, self.half_depth)];
        for k in 0..=24 {
            let theta = std::f64::consts::PI + std::f64::consts::PI * f64::from(k) / 24.0;
            let r = reach(theta);
            line.push(self.at(theta.cos() * r, theta.sin() * r));
        }
        line.push(self.at(side, self.half_depth));
        line
    }
}

/// The bend park's frame: between the free side lot lines of the lots either side of it,
/// from Neblina's sidewalk to its back wall ([`park_corners`]).
fn bend_park(lines: &Lines) -> Option<Park> {
    let (a, b) = park_corners(&lines.lots)?;
    let sidewalk = |corner: DVec2| {
        let (point, along) = lines.gaviota.at(lines.gaviota.along(corner));
        let side = lines.gaviota.signed_distance(corner).signum();
        point + left(along) * side * STREET_HALF_WALK
    };
    let (front_a, front_b) = (sidewalk(a), sidewalk(b));
    let corners = [front_a, a, b, front_b];
    let middle = corners.iter().copied().sum::<DVec2>() / 4.0;
    let out = ((front_a + front_b) / 2.0 - (a + b) / 2.0).normalize();
    let across = left(out);
    let half_width = (front_a - front_b)
        .dot(across)
        .abs()
        .min((a - b).dot(across).abs())
        / 2.0;
    let half_depth = ((front_a + front_b) / 2.0 - middle).dot(out);
    Some(Park {
        middle,
        out,
        across,
        corners,
        half_width,
        half_depth,
    })
}

/// The bend park's ground: bark mulch between its lot lines, its kidney lawn and the
/// concrete walk curving round it from the sidewalk and back.
fn bend_park_grounds(group: &mut Node, lines: &Lines) {
    let Some(park) = bend_park(lines) else {
        return;
    };
    group
        .children
        .push(flat(quad(park.corners, 0.026), PARK_MULCH));
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    const SEGMENTS: usize = 36;
    let rim = |k: usize| {
        let theta = std::f64::consts::TAU * k as f64 / SEGMENTS as f64;
        let r = park.lawn(theta);
        park.at(theta.cos() * r, theta.sin() * r)
    };
    for k in 0..SEGMENTS {
        let (a, b) = (rim(k), rim(k + 1));
        let middle = park.middle;
        let turn = (a.y - middle.y) * (b.x - middle.x) - (a.x - middle.x) * (b.y - middle.y);
        let triangle = if turn > 0.0 {
            [middle, a, b]
        } else {
            [middle, b, a]
        };
        for p in triangle {
            positions.extend([p.x, 0.036, p.y]);
            normals.extend([0.0, 1.0, 0.0]);
            uvs.extend([p.x / 4.0, p.y / 4.0]);
        }
    }
    group.children.push(flat(
        Mesh::from_f64(&positions, &normals, &uvs, None),
        PARK_GRASS,
    ));
    let nothing = |_: DVec2| false;
    group.children.push(flat(
        ribbon(
            &park.walk(),
            -PARK_WALK / 2.0,
            PARK_WALK / 2.0,
            0.05,
            &nothing,
        ),
        SIDEWALK_CONCRETE,
    ));
}

/// The bend park's furniture: two benches on the walk's outer edge behind the lawn
/// facing it, a bank of cluster mailboxes by the sidewalk, and a tall lamp at the walk's
/// mouth.
fn bend_park_props(group: &mut Node, lines: &Lines, globe: &Arc<Mesh>) {
    let Some(park) = bend_park(lines) else {
        return;
    };
    let yaw_of = |facing: DVec2| facing.x.atan2(facing.y);
    for theta in [
        1.5 * std::f64::consts::PI - 0.55,
        1.5 * std::f64::consts::PI + 0.55,
    ] {
        let r = park.lawn(theta) + PARK_WALK + 0.55;
        let at = park.at(theta.cos() * r, theta.sin() * r);
        let facing = (park.middle - at).normalize();
        let yaw = yaw_of(facing);
        let (length, seat) = BENCH;
        let piece = |node: Node| rotated(node, 0.0, yaw, 0.0);
        put(
            group,
            piece(box_part(length, 0.06, 0.42, BENCH_WOOD, 0.0)),
            at.x,
            seat,
            at.y,
        );
        let back = at - facing * 0.2;
        put(
            group,
            piece(box_part(length, 0.4, 0.05, BENCH_WOOD, 0.0)),
            back.x,
            seat + 0.25,
            back.y,
        );
        for end in [-1.0, 1.0] {
            let leg = at + left(facing) * end * (length / 2.0 - 0.1);
            put(
                group,
                piece(box_part(0.06, seat, 0.4, LAMP_METAL, 0.0)),
                leg.x,
                seat / 2.0,
                leg.y,
            );
        }
    }
    let mailbox = park.at(park.half_width * 0.15, park.half_depth - 1.1);
    mailbox_bank(group, mailbox, park.out);
    let side = park.lawn(0.0) + PARK_WALK + 0.45;
    let (x, z) = park.at(-side, park.half_depth - 0.6).into();
    lamp(group, globe, (x, z), WAY_LAMP_HEIGHT, WAY_LAMP_GLOBE);
}

/// Whether `p` lies in the bend park's planting beds: inside it, off its lawn and walk.
fn in_park_beds(park: &Park, p: DVec2) -> bool {
    let local = p - park.middle;
    let (u, v) = (local.dot(park.across), local.dot(park.out));
    let theta = v.atan2(u);
    let r = u.hypot(v);
    let walk = Line(park.walk());
    u.abs() < park.half_width - 0.3
        && v.abs() < park.half_depth - 0.6
        && r > park.lawn(theta) + 0.25
        && walk.distance(p) > PARK_WALK / 2.0 + 0.25
}

/// The dark mulch basins along the bank's crest beside Senda Parkway's path, half discs
/// opening down the bank, wherever the bank is wide enough.
fn crest_basins(group: &mut Node, lines: &Lines) {
    for (middle, down) in crest_basin_sites(lines) {
        let (radius, _, paint) = CREST_BASIN;
        let across = left(down);
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        const SEGMENTS: usize = 8;
        let rim = |k: usize| {
            let angle = std::f64::consts::PI * k as f64 / SEGMENTS as f64;
            middle + (across * angle.cos() + down * angle.sin()) * radius
        };
        for k in 0..SEGMENTS {
            let (a, b) = (rim(k), rim(k + 1));
            let turn = (a.y - middle.y) * (b.x - middle.x) - (a.x - middle.x) * (b.y - middle.y);
            let triangle = if turn > 0.0 {
                [middle, a, b]
            } else {
                [middle, b, a]
            };
            for p in triangle {
                positions.extend([p.x, 0.034, p.y]);
                normals.extend([0.0, 1.0, 0.0]);
                uvs.extend([p.x / 4.0, p.y / 4.0]);
            }
        }
        group.children.push(flat(
            Mesh::from_f64(&positions, &normals, &uvs, None),
            paint,
        ));
    }
}

/// The crest basins' middles on the bank's top edge, just down from Senda Parkway's path,
/// and the way down the bank from each: one every [`CREST_BASIN`] spacing where the bank
/// between the path and Rumbo's back walls is wide enough, inside the arena.
fn crest_basin_sites(lines: &Lines) -> Vec<(DVec2, DVec2)> {
    let way = &lines.way;
    let top = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + 0.15;
    let mut sites = Vec::new();
    let mut s = 0.0;
    while s < way.length() {
        let (point, along) = way.at(s);
        // The bank lies to the Way's right (south).
        let down = -left(along);
        let at = point + down * top;
        // Down to the back walls of Rumbo's north row, at the bank's foot.
        let bank = -lines.lot(at).0 - WALL_FOOT;
        if bank > BANK_TRAIL_ROOM && in_arena(at) && at.x < HILLSIDE_FROM {
            sites.push((at, down));
        }
        s += CREST_BASIN.1;
    }
    sites
}

/// The lots' back-yard furniture, scenery a tank pushes through (the loggias and
/// pergolas over the patios are covers, [`patio_cover`]): a round spa in a back corner of some, a fire pit out on the
/// yard of others, a trampoline on the turf of a few, a patio umbrella and a potted tree;
/// the planted beds of tall grass outside the rows' end lots; and the bend park's benches, mailboxes
/// and lamp.
fn yard_props() -> Node {
    let mut group = Node::group("senda-yards");
    let lines = Lines::new();
    let bush = Arc::new(heart_geometry());
    let canopy = Arc::new(cylinder_geometry(0.04, UMBRELLA.0, 0.32, 8));
    for (k, lot) in lines.lots.iter().enumerate() {
        let yaw = lot.front.x.atan2(lot.front.y);
        let part = |group: &mut Node, node: Node, (u, v): (f64, f64), y: f64| {
            let at = lot.at(u, v);
            put(group, rotated(node, 0.0, yaw, 0.0), at.x, y, at.y);
        };
        let back = house_back(lot);
        let depth = patio_depth(lot);
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let side = turf_side(k);
        // A patio umbrella on the patio's open side.
        if k % 5 == 1 {
            let (radius, pole) = UMBRELLA;
            let at = (-side * (lot.half_width - radius - 0.1), back - depth / 2.0);
            part(
                &mut group,
                cylinder_part(0.025, pole, LAMP_METAL, 5),
                at,
                pole / 2.0,
            );
            let paint_of = if k % 2 == 0 {
                UMBRELLA_CANVAS.0
            } else {
                UMBRELLA_CANVAS.1
            };
            part(
                &mut group,
                shadowed(canopy.clone(), paint(paint_of)),
                at,
                pole - 0.1,
            );
        }
        let corner = |side: f64, inset: f64| {
            let edge = if side > 0.0 { u1 } else { u0 };
            (edge - side * inset, lot.rear_v() + inset)
        };
        // The trampoline on the turf, in the turf side's back corner.
        let (radius, height) = TRAMPOLINE;
        if k % TRAMPOLINE_EVERY == 4 && lot.yard - depth > 1.0 && lot.yard > 2.0 * radius + 0.6 {
            let at = corner(side, radius + 0.25);
            part(
                &mut group,
                cylinder_part(radius + 0.08, 0.07, TRAMPOLINE_PAD, 16),
                at,
                height - 0.03,
            );
            part(
                &mut group,
                cylinder_part(radius - 0.05, 0.03, TRAMPOLINE_MAT, 16),
                at,
                height + 0.01,
            );
            for n in 0..4 {
                let turn = std::f64::consts::FRAC_PI_4 + FRAC_PI_2 * f64::from(n);
                let leg = (at.0 + turn.cos() * radius, at.1 + turn.sin() * radius);
                part(
                    &mut group,
                    box_part(0.04, height, 0.04, TRAMPOLINE_MAT, 0.0),
                    leg,
                    height / 2.0,
                );
            }
        }
        // A round spa or a potted tree in the other back corner.
        let (spa, spa_height) = SPA;
        if k % SPA_EVERY == 3 && lot.yard > 2.0 * spa + 0.4 {
            let at = corner(-side, spa + 0.2);
            part(
                &mut group,
                cylinder_part(spa, spa_height, SPA_COPING, 16),
                at,
                spa_height / 2.0,
            );
            part(
                &mut group,
                cylinder_part(spa - 0.12, 0.02, SPA_WATER, 16),
                at,
                spa_height + 0.002,
            );
        } else if k % 3 == 0 {
            let (radius, height, pot) = POT;
            let at = corner(-side, 0.45);
            part(
                &mut group,
                cylinder_part(radius, height, pot, 10),
                at,
                height / 2.0,
            );
            part(
                &mut group,
                cylinder_part(0.04, POT_CROWN.1, 0x5a4634, 5),
                at,
                height + POT_CROWN.1 / 2.0,
            );
            let tone = PLANTED_LEAVES[k % PLANTED_LEAVES.len()];
            let spot = lot.at(at.0, at.1);
            let base = height + POT_CROWN.1 - POT_CROWN.0;
            leafy(
                &mut group,
                &bush,
                (spot, base),
                (POT_CROWN.0, 1.1),
                tone,
                k as f64 * 2.9,
            );
        }
        // A small tree in the turf side's back corner (but where the trampoline stands),
        // and low shrubs along the back bed, clear of it and of the other corner's spa
        // or potted tree.
        let tree = k % TRAMPOLINE_EVERY != 4;
        if tree {
            let (crown, height, leaves) = YARD_TREE[k % YARD_TREE.len()];
            let (u, v) = corner(side, 0.75);
            let at = lot.at(u, v);
            put(
                &mut group,
                cylinder_part(0.05, height - crown * 0.4, 0x5a4634, 5),
                at.x,
                (height - crown * 0.4) / 2.0,
                at.y,
            );
            shrub_at(
                &mut group,
                &bush,
                (at, height - crown * 0.45),
                (crown / 2.0, 0.85),
                3,
                &[leaves, darker(leaves, 0.85)],
                k as f64 * 3.3,
            );
        }
        let other_taken = k % SPA_EVERY == 3 || k % 3 == 0;
        let mut u = u0 + 0.5;
        let mut n = 0usize;
        while u < u1 - 0.4 {
            let seed = (k * 41 + n) as f64 * 1.3;
            let toward = |side: f64| if side > 0.0 { u1 - u } else { u - u0 };
            let clear = (!tree || toward(side) > 1.5) && (!other_taken || toward(-side) > 1.7);
            if clear {
                let (least, most) = YARD_SHRUB;
                let at = lot.at(
                    u,
                    lot.rear_v() + 0.1 + YARD_PLANTER * (0.4 + 0.2 * hash(seed)),
                );
                shrub(
                    &mut group,
                    &bush,
                    at,
                    least + (most - least) * hash(seed + 0.4),
                    0.8,
                    2,
                    &YARD_SHRUB_LEAVES,
                    seed,
                );
            }
            u += YARD_SHRUB_STEP;
            n += 1;
        }
        // Dark shrubs along both side walls' beds, from the back bed's corners to the
        // patio.
        let (from, to) = (lot.rear_v() + 0.1 + YARD_PLANTER + 0.5, back - depth - 0.3);
        for (edge, end) in [
            (u0 + 0.1 + SIDE_PLANTER / 2.0, 0),
            (u1 - 0.1 - SIDE_PLANTER / 2.0, 1),
        ] {
            let mut v = from;
            while v < to {
                let seed = (k * 29 + end * 7) as f64 + v * 3.3;
                let (least, most) = SIDE_SHRUB;
                let tone = SIDE_SHRUB_LEAVES[(hash(seed) * 3.0) as usize % 3];
                let size = least + (most - least) * hash(seed + 0.5);
                leafy(
                    &mut group,
                    &bush,
                    (lot.at(edge, v), 0.0),
                    (size, 0.9),
                    tone,
                    seed,
                );
                v += SIDE_SHRUB_STEP;
            }
        }
        // A fire pit out on the yard past the patio, on the open side.
        let (pit, pit_height) = FIRE_PIT;
        let open = lot.yard - depth - YARD_PLANTER;
        if k % 3 == 1 && k % SPA_EVERY != 3 && open > 2.0 * pit + 0.3 {
            let at = (-side * (lot.half_width * 0.35), back - depth - open / 2.0);
            part(
                &mut group,
                cylinder_part(pit, pit_height, FIRE_PIT_STONE, 12),
                at,
                pit_height / 2.0,
            );
            part(
                &mut group,
                cylinder_part(pit - 0.1, 0.02, FIRE_PIT_ASH, 12),
                at,
                pit_height + 0.005,
            );
        }
    }
    end_lot_beds(&mut group, &lines);
    let globe = Arc::new(sphere_geometry(1.0, 8, 6));
    bend_park_props(&mut group, &lines, &globe);
    batch_in_cells(&mut group, 40.0);
    group
}

/// The planted bed outside each row's end lot's side view fence, toward the street or the
/// hillside: blue-grey groundcover under two staggered lines of tall ornamental grass, in
/// pieces that leave out the paving, the other lots, the spawn pads and the pickups.
fn end_lot_beds(group: &mut Node, lines: &Lines) {
    let (width, piece) = SIDE_BED;
    let pads: Vec<DVec2> = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    let open = |p: DVec2| {
        !is_senda_paved(p.x, p.y)
            && lines.lot(p).0 < -0.05
            && pads.iter().all(|pad| pad.distance(p) > SHRUB_PAD_ROOM)
            && SENDA_PICKUPS
                .iter()
                .all(|pickup| DVec2::new(pickup.x, pickup.z).distance(p) > SHRUB_PICKUP_ROOM)
    };
    for (n, lot) in lines.lots.iter().enumerate() {
        for (side, sign) in lot.sides.iter().zip([-1.0, 1.0]) {
            if side.neighbour.is_some() {
                continue;
            }
            let out = lot.across * sign;
            let fence = YARD_WALL_THICKNESS / 2.0 + 0.05;
            let (a, b) = (side.rear, side.gate);
            let pieces = (a.distance(b) / piece).floor() as usize;
            let along = (b - a).normalize_or_zero();
            for k in 0..pieces {
                let start = a + along * (piece * k as f64);
                let end = start + along * piece;
                let corners = [
                    start + out * fence,
                    end + out * fence,
                    end + out * (fence + width),
                    start + out * (fence + width),
                ];
                if !corners.into_iter().all(open) {
                    continue;
                }
                group
                    .children
                    .push(flat(quad(corners, 0.04), SIDE_BED_COVER));
                for line in 0..2 {
                    let shift = if line == 0 { 0.0 } else { SIDE_BED_TUFT / 2.0 };
                    let reach = fence + width * (0.3 + 0.4 * line as f64);
                    let mut t = shift + SIDE_BED_TUFT / 2.0;
                    while t < piece {
                        let seed = (n * 97 + k * 11 + line) as f64 + t * 3.1;
                        let at = start
                            + along * (t + (hash(seed) - 0.5) * 0.2)
                            + out * (reach + (hash(seed + 0.3) - 0.5) * 0.2);
                        tall_grass(group, at, seed);
                        t += SIDE_BED_TUFT;
                    }
                }
            }
        }
    }
}

/// A tuft of tall ornamental grass at `at`: a fountain of long narrow blades leaning out
/// from its foot.
fn tall_grass(group: &mut Node, at: DVec2, seed: f64) {
    let (_, low, high) = TALL_GRASS;
    let height = low + (high - low) * hash(seed + 1.7);
    grass_blades(group, at, height, &TALL_GRASS_LEAVES, seed);
}

/// A fountain of long narrow blades `height` tall leaning out from `at`, in `leaves`.
fn grass_blades(group: &mut Node, at: DVec2, height: f64, leaves: &[u32], seed: f64) {
    let blade = TALL_GRASS.0;
    for n in 0..TALL_GRASS_BLADES {
        let k = seed + n as f64 * 1.9;
        let turn = n as f64 / TALL_GRASS_BLADES as f64 * std::f64::consts::TAU + hash(k);
        // Leaning out from upright, the inner blades the least.
        let lean = 0.12 + 0.4 * hash(k + 0.4);
        let length = height * (0.75 + 0.25 * hash(k + 0.8)) / lean.cos();
        let tone = leaves[(hash(k + 2.3) * leaves.len() as f64) as usize % leaves.len()];
        // Tipped up from lying along +z to `lean` off upright, then turned to face out.
        let mut part = box_part(blade, 0.02, length, tone, 0.0);
        part.rotation = DQuat::from_rotation_y(turn) * DQuat::from_rotation_x(-(FRAC_PI_2 - lean));
        let reach = at + DVec2::new(turn.sin(), turn.cos()) * lean.sin() * length / 2.0;
        put(group, part, reach.x, lean.cos() * length / 2.0, reach.y);
    }
}

/// A red aloe at `at`: a low rosette of grey-green leaves under a few orange-red flower
/// spikes.
fn aloe(group: &mut Node, lump: &Arc<Mesh>, at: DVec2, seed: f64) {
    let (size, leaves) = ALOE;
    agave_of(group, lump, (at, 0.0), (size, hash(seed) * 6.0), leaves);
    let (spike, flower) = ALOE_SPIKE;
    for n in 0..3 {
        let turn = n as f64 * 2.1 + hash(seed + 0.5);
        let spot = at + DVec2::new(turn.cos(), turn.sin()) * 0.08;
        let tall = spike * (0.8 + 0.3 * hash(seed + n as f64));
        put(
            group,
            cylinder_part(0.012, tall * 0.65, 0x6a6a3e, 4),
            spot.x,
            tall * 0.65 / 2.0,
            spot.y,
        );
        let mut bloom = shadowed(lump.clone(), paint(flower));
        bloom.scale = glam::DVec3::new(0.05, tall * 0.2, 0.05);
        put(group, bloom, spot.x, tall * 0.8, spot.y);
    }
}

/// Oaks and sycamores along Senda Parkway outside the walls, the band of oak and ficus
/// crowns down the slope behind Gaviota's south row past the level's wall, the
/// hillside's oaks in clusters past the arena's east edge, and their sage.
fn slope_trees() -> Node {
    let mut group = Node::group("senda-slope-trees");
    let mut place = |x: f64, z: f64, (w, h): (f64, f64)| {
        let (x, z) = oak_site(x, z);
        let shape = TreeShape { x, z, w, d: w, h };
        put(
            &mut group,
            tree_model(&shape, TreeDetail::Background),
            x,
            hill_height(x, z),
            z,
        );
    };
    let size = |k: usize| (2.6, 6.0 + (k % 3) as f64 * 0.6);
    // Down Senda Parkway's median and parkways outside the level's wall (inside it they
    // are cover).
    for (k, at) in outer_way_tree_sites().into_iter().enumerate() {
        place(at.x, at.y, size(k));
    }
    // Along Senda Parkway, either side of it, wherever it runs outside the walls.
    let way = Street::SendaParkway.line();
    let mut k = 0;
    let mut s = 0.0;
    while s < way.length() {
        let (point, along) = way.at(s);
        for side in [-1.0, 1.0] {
            let wobble = ((s * 0.37).sin() * 2.0).round();
            let at = point + left(along) * side * (Street::SendaParkway.half() + 5.0 + wobble);
            if at.x.abs().max(at.y.abs()) > ARENA + 4.0 {
                place(at.x, at.y, size(k));
            }
            k += 1;
        }
        s += SLOPE_TREE_SPACING;
    }
    // The slope down to Gardenia behind Gaviota's south row and Senda Parkway's west branch
    // behind Neblina's west row: overlapping crowns in staggered rows from the level's
    // wall outward, so they close the view south and west as the street trees and
    // the slope's do.
    let boundary = senda_boundary();
    let gaviota = Street::NeblinaGaviota.line();
    let way_reach = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + 2.0;
    let rows = house_front() + HOUSE_DEPTH;
    let outside = |p: DVec2, room: f64| {
        (0..8).all(|n| {
            let angle = f64::from(n) * std::f64::consts::FRAC_PI_4;
            !inside_boundary(&boundary, p + DVec2::new(angle.cos(), angle.sin()) * room)
        })
    };
    let near_wall = |p: DVec2| {
        (0..12).any(|n| {
            let angle = f64::from(n) * std::f64::consts::TAU / 12.0;
            inside_boundary(
                &boundary,
                p + DVec2::new(angle.cos(), angle.sin()) * CANOPY_BAND,
            )
        })
    };
    let (spacing, (least, most), (low, high)) = (CANOPY_SPACING, CANOPY_CROWN, CANOPY_HEIGHT);
    let mut row = 0;
    let mut z = -ARENA - CANOPY_BAND;
    while z < ARENA + CANOPY_BAND {
        let mut x = -ARENA - CANOPY_BAND + if row % 2 == 0 { 0.0 } else { spacing / 2.0 };
        while x < ARENA + 2.0 {
            let seed = x * 2.17 + z * 3.31;
            let at = DVec2::new(
                x + (hash(seed) - 0.5) * spacing * 0.5,
                z + (hash(seed + 0.4) - 0.5) * spacing * 0.5,
            );
            if gaviota.signed_distance(at) < -rows
                && way.signed_distance(at) < -way_reach
                && outside(at, BOUNDARY_THICKNESS + CANOPY_WALL_ROOM)
                && near_wall(at)
            {
                let crown = least + (most - least) * hash(seed + 1.7);
                let height = low + (high - low) * hash(seed + 2.9);
                place(at.x, at.y, (crown, height));
            }
            x += spacing;
        }
        z += spacing * 0.87;
        row += 1;
    }
    // Inside the west wall, small trees between the spawn pads.
    let pads: Vec<DVec2> = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    let mut west: Vec<f64> = pads
        .iter()
        .filter(|pad| pad.x < 0.0)
        .map(|pad| pad.y)
        .collect();
    west.sort_by(f64::total_cmp);
    let lots = senda_lots(&senda_houses());
    let clear_of = |at: DVec2, crown: f64| {
        pads.iter()
            .all(|pad| pad.distance(at) > SHRUB_PAD_ROOM + crown / 2.0)
            && SENDA_PICKUPS.iter().all(|pickup| {
                DVec2::new(pickup.x, pickup.z).distance(at) > SHRUB_PICKUP_ROOM + crown / 2.0
            })
            && lots.iter().all(|lot| lot.inside(at) < -crown / 2.0)
            && inside_boundary(&boundary, at + DVec2::new(-crown / 2.0, crown / 2.0))
            && inside_boundary(&boundary, at + DVec2::new(crown / 2.0, crown / 2.0))
    };
    let small = |seed: f64| {
        let (least, most) = WEST_TREE_CROWN;
        let (low, high) = WEST_TREE_HEIGHT;
        (
            least + (most - least) * hash(seed),
            low + (high - low) * hash(seed + 0.6),
        )
    };
    let mut sites: Vec<(DVec2, f64)> = Vec::new();
    for gap in west.windows(2) {
        for share in [1.0 / 3.0, 2.0 / 3.0] {
            let z = gap[0] + (gap[1] - gap[0]) * share;
            sites.push((DVec2::new(-ARENA + WEST_TREE_INSET, z), z * 1.7));
        }
    }
    // Behind Gaviota's south row the level's band of oaks (`senda_layout::big_trees`)
    // stands 2 and 5 m off the back walls, so no small trees go there too.
    for (at, seed) in sites {
        let (crown, height) = small(seed);
        let (x, z) = oak_site(at.x, at.y);
        if clear_of(DVec2::new(x, z), crown) {
            place(x, z, (crown, height));
        }
    }
    // The hillside east of the arena: oaks in clusters of three to five among its sage.
    let mut z = -ARENA;
    let mut k = 0;
    while z < ARENA + 10.0 {
        for column in 0..1 {
            let seed = z * 1.37 + f64::from(column) * 11.0;
            let middle = DVec2::new(
                ARENA + 12.0 + f64::from(column) * 16.0 + (hash(seed) - 0.5) * 6.0,
                z + (hash(seed + 0.3) - 0.5) * 8.0,
            );
            let count = 3 + (hash(seed + 0.9) * 2.0) as usize;
            for n in 0..count {
                let turn = hash(seed + n as f64 * 1.9) * std::f64::consts::TAU;
                let reach = if n == 0 {
                    0.0
                } else {
                    2.2 + 1.4 * hash(seed + n as f64)
                };
                let at = middle + DVec2::new(turn.cos(), turn.sin()) * reach;
                place(
                    at.x,
                    at.y,
                    (2.4 + 0.8 * hash(seed + n as f64 * 2.3), size(k).1 - 0.6),
                );
                k += 1;
            }
        }
        z += HILLSIDE_OAK_SPACING;
    }
    sage_clumps(&mut group);
    batch_in_cells(&mut group, 40.0);
    group
}

/// How dark a leafy shrub's heart is beside its leaves.
const LEAFY_HEART: f64 = 0.72;

/// A kind of shrub clump: its grid's spacing (m), its leaves, its lumps' size range
/// and how tall a lump stands for its breadth.
struct Clumps {
    spacing: f64,
    leaves: &'static [u32],
    lump: (f64, f64),
    squash: f64,
    /// Leafy shrubs rather than painted lumps (the grasses' tufts).
    leafy: bool,
}

const SAGE_CLUMPS: Clumps = Clumps {
    spacing: SAGE_SPACING,
    leaves: &SAGE_LEAVES,
    lump: SAGE_LUMP,
    squash: 0.75,
    leafy: true,
};
const PLANTED_CLUMPS: Clumps = Clumps {
    spacing: PLANTED_SPACING,
    leaves: &PLANTED_LEAVES,
    lump: PLANTED_LUMP,
    squash: 0.6,
    leafy: true,
};
const GRASS_CLUMPS: Clumps = Clumps {
    spacing: GRASS_SPACING,
    leaves: &GRASS_LEAVES,
    lump: GRASS_LUMP,
    squash: 1.1,
    leafy: false,
};

/// Clumps of two or three lumps scattered off a grid over the rectangle from `low` to
/// `high` by a slow wobble, wherever `wanted` holds of a clump's middle, standing on
/// ground `base` metres up.
fn clumps(
    group: &mut Node,
    lump: &Arc<Mesh>,
    kind: &Clumps,
    (low, high): (DVec2, DVec2),
    base: f64,
    wanted: &dyn Fn(DVec2) -> bool,
) {
    let jitter = |a: f64| (a * 12.9898).sin() * 43_758.545;
    let mut k = 0usize;
    let mut x = low.x;
    while x < high.x {
        let mut z = low.y;
        while z < high.y {
            let (dx, dz) = (jitter(x + z * 0.7).fract(), jitter(z - x * 0.3).fract());
            let (cx, cz) = (x + dx * kind.spacing * 0.45, z + dz * kind.spacing * 0.45);
            if wanted(DVec2::new(cx, cz)) {
                let (least, most) = kind.lump;
                for n in 0..2 + k % 2 {
                    let size = least + (most - least) * jitter(cx + n as f64).fract().abs();
                    let leaves = kind.leaves[(k + n) % kind.leaves.len()];
                    let offset = n as f64 * size * 0.65;
                    if kind.leafy {
                        let spot = DVec2::new(cx + offset, cz - offset * 0.6);
                        let seed = cx * 3.1 + cz * 1.7 + n as f64;
                        leafy(group, lump, (spot, base), (size, kind.squash), leaves, seed);
                        continue;
                    }
                    let spot = DVec2::new(cx + offset, cz - offset * 0.6);
                    let seed = cx * 2.3 + cz * 4.1 + n as f64;
                    tuft(group, spot, size, kind.squash, &[leaves], seed);
                }
            }
            k += 1;
            z += kind.spacing;
        }
        x += kind.spacing;
    }
}

/// Low coastal sage over the hillside past the arena's east edge, in patches round its
/// oaks with natural grass between.
fn sage_clumps(group: &mut Node) {
    let lump = Arc::new(heart_geometry());
    let low = DVec2::new(ARENA + 3.0, -ARENA - 20.0);
    let high = DVec2::new(ARENA + 40.0, ARENA + 20.0);
    let dense = Clumps {
        spacing: OUTER_SAGE_SPACING,
        ..SAGE_CLUMPS
    };
    clumps(group, &lump, &dense, (low, high), 0.0, &|p| {
        hill_patch(p) > 0.5
    });
}

/// A shrub's lump: a ball of eighty faces, its surface pushed in and out a little from
/// spot to spot so no two shrubs read as the same crisp polyhedron, shaded smooth.
fn bush_geometry() -> Mesh {
    let mut mesh = icosahedron_geometry(1.0, 1);
    for (p, n) in mesh.positions.iter_mut().zip(mesh.normals.iter_mut()) {
        let v = glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
        let key = (v * 7.0).round();
        let bump = 0.82 + 0.3 * hash(key.x * 1.7 + key.y * 3.1 + key.z * 5.3);
        *n = crate::geometry::narrow(v.normalize());
        *p = crate::geometry::narrow(v * bump);
    }
    mesh
}

/// A leafy shrub's dark heart: a low-poly ball, shaded smooth, hidden among its cards.
fn heart_geometry() -> Mesh {
    let mut mesh = icosahedron_geometry(1.0, 0);
    for (p, n) in mesh.positions.iter().zip(mesh.normals.iter_mut()) {
        let v = glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
        *n = crate::geometry::narrow(v.normalize());
    }
    mesh
}

/// Whether `p` lies on or within a metre of a house's walls.
fn by_a_house(p: DVec2, houses: &[SendaHouse]) -> bool {
    houses.iter().any(|house| {
        let design = house.design();
        let local = design.to_local(glam::DVec3::new(p.x - house.x, 0.0, p.y - house.z));
        local.x.abs() < design.w / 2.0 + 1.0 && local.z.abs() < design.d / 2.0 + 1.0
    })
}

/// `color` with each channel scaled by `factor`.
fn darker(color: u32, factor: f64) -> u32 {
    let channel = |shift: u32| {
        let value = f64::from((color >> shift) & 0xff) * factor;
        (value.round() as u32).min(0xff) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// A fraction from 0 to 1 that follows from `a` alone, for jitter that must not touch
/// the game's random stream.
fn hash(a: f64) -> f64 {
    ((a * 12.9898).sin() * 43_758.545).fract().abs()
}

/// A tuft of ornamental grass at `at` about `size` across, standing `squash` times as
/// tall: a fountain of narrow blades in `leaves`.
fn tuft(group: &mut Node, at: DVec2, size: f64, squash: f64, leaves: &[u32], seed: f64) {
    grass_blades(group, at, size * squash * TUFT_HEIGHT, leaves, seed);
}

/// One leafy shrub on ground `base` metres up at `at`, `size` metres in radius and
/// `squash` as tall for its breadth, in `color`: small-leaved sprig cards shaded as one
/// soft mound ([`leafy_shrub`]) round a darker heart on `core` that keeps the mass dense
/// where the cards' gaps would show the ground through.
fn leafy(
    group: &mut Node,
    core: &Arc<Mesh>,
    (at, base): (DVec2, f64),
    (size, squash): (f64, f64),
    color: u32,
    seed: f64,
) {
    let radii = glam::DVec3::new(size, size * squash, size);
    let middle = glam::DVec3::new(at.x, base + radii.y * 0.8, at.y);
    let mut heart = shadowed(core.clone(), paint(darker(color, LEAFY_HEART)));
    heart.scale = radii * 0.62;
    heart.set_rotation_euler(0.0, hash(seed + 0.21) * 3.0, 0.0);
    put(group, heart, middle.x, middle.y - radii.y * 0.12, middle.z);
    let pick = (hash(seed + 0.37) * 1.0e6) as u32;
    group.children.push(leafy_shrub(middle, radii, color, pick));
}

/// One shrub of `lumps` leafy masses round `at` of about `size`, each squashed by
/// `squash` and set unevenly so the mass reads as an irregular mound, not a ball, in
/// `leaves`; `core` is the dark heart under each mass's leaves.
#[allow(clippy::too_many_arguments)]
fn shrub(
    group: &mut Node,
    core: &Arc<Mesh>,
    at: DVec2,
    size: f64,
    squash: f64,
    lumps: usize,
    leaves: &[u32],
    seed: f64,
) {
    // Two masses at most: past that the cards crowd into one blur.
    for n in 0..lumps.min(2) {
        let k = seed + n as f64 * 3.7;
        let r = size * (0.75 + 0.3 * hash(k));
        let angle = hash(k + 1.3) * std::f64::consts::TAU;
        let reach = if n == 0 {
            0.0
        } else {
            size * (0.5 + 0.3 * hash(k + 2.1))
        };
        let spot = at + DVec2::new(angle.cos(), angle.sin()) * reach;
        let tone = leaves[(hash(k + 4.4) * leaves.len() as f64) as usize % leaves.len()];
        leafy(group, core, (spot, 0.0), (r, squash), tone, k);
    }
}

/// A shrub like [`shrub`]'s whose lumps stand round `at` on a base `base` metres up: a
/// small tree's crown on its trunk.
fn shrub_at(
    group: &mut Node,
    lump: &Arc<Mesh>,
    (at, base): (DVec2, f64),
    (size, squash): (f64, f64),
    lumps: usize,
    leaves: &[u32],
    seed: f64,
) {
    let mut crown = Node::default();
    shrub(&mut crown, lump, at, size, squash, lumps, leaves, seed);
    for mut part in crown.children {
        part.position.y += base;
        group.children.push(part);
    }
}

/// Shrubs set out in contour rows along `line` on its `side` (1 its left), the rows from
/// `from` to `to` metres out, wherever `wanted` holds of a shrub's spot: about one size,
/// in `leaves`, each a single round leafy shrub on `lump`, its heart.
#[allow(clippy::too_many_arguments)]
fn contour_rows(
    group: &mut Node,
    lump: &Arc<Mesh>,
    line: &Line,
    side: f64,
    (from, to): (f64, f64),
    leaves: &[u32],
    wanted: &dyn Fn(DVec2) -> bool,
) {
    let mut row = 0usize;
    let mut out = from;
    while out < to {
        let mut s = if row.is_multiple_of(2) {
            0.0
        } else {
            CONTOUR_STEP / 2.0
        };
        while s < line.length() {
            let (point, along) = line.at(s);
            let site = point + left(along) * side * out;
            let seed = site.x * 1.31 + site.y * 7.17;
            let at = site
                + (along * (hash(seed + 7.7) - 0.5) * CONTOUR_STEP
                    + left(along) * (hash(seed + 8.3) - 0.5) * CONTOUR_ROW)
                    * CONTOUR_JITTER;
            if hash(seed + 9.1) > CONTOUR_BARE && wanted(at) {
                let (least, most) = CONTOUR_SIZE;
                let size = least + (most - least) * hash(seed);
                let tone = leaves[(hash(seed + 3.9) * leaves.len() as f64) as usize % leaves.len()];
                leafy(group, lump, (at, -0.05), (size, 0.78), tone, seed);
            }
            s += CONTOUR_STEP;
        }
        out += CONTOUR_ROW;
        row += 1;
    }
}

/// The shrubs: the bank under Senda Parkway
/// planted in contour rows on their mulch, the planted slope at the greenbelt's mouth,
/// the bank's crest basins and the drifts along the hillside trails, the hillside's sage
/// in patches, the ornamental grasses down Senda Parkway's median and on the corner beds
/// where Rumbo and the greenbelt meet Neblina and Neblina leaves Senda Parkway, the
/// clipped hedge along the Way's south parkway, all clear of the paving, the walled lots,
/// the spawn pads and the pickups (scenery, not cover: a tank pushes through them), the
/// houses' entry beds and the low shrubs along their fronts, the bend park's beds, the
/// agaves at the cul-de-sacs' ends.
fn shrubs() -> Node {
    let mut group = Node::group("senda-shrubs");
    let lump = Arc::new(octahedron_geometry(1.0, 1));
    let bush = Arc::new(heart_geometry());
    let ball = Arc::new(bush_geometry());
    let lines = Lines::new();
    let houses = senda_houses();
    let pads: Vec<DVec2> = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    let streets: Vec<(f64, Line)> = Street::ALL
        .into_iter()
        .map(|street| (street.half(), street.line()))
        .collect();
    let way_reach = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + SHRUB_PAVING_ROOM;
    let open = |p: DVec2| {
        let paved = is_senda_paved(p.x, p.y)
            || streets.iter().any(|(half, line)| {
                line.distance(p) < half + PARKWAY + SIDEWALK + SHRUB_PAVING_ROOM
            })
            || lines.way.distance(p) < way_reach;
        !paved
            && pads.iter().all(|pad| pad.distance(p) > SHRUB_PAD_ROOM)
            && SENDA_PICKUPS
                .iter()
                .all(|pickup| DVec2::new(pickup.x, pickup.z).distance(p) > SHRUB_PICKUP_ROOM)
    };
    // Off the lots, by more than a shrub's reach past their walls.
    let off_lots = |p: DVec2, room: f64| lines.lot(p).0 < -room;
    let in_arena = |p: DVec2| p.x.abs().max(p.y.abs()) < ARENA - 0.6;
    // The bank below Senda Parkway, in rows down from its crest basins to the mulch along
    // the back walls; and the slope across the Way, inside the arena.
    let top = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH;
    contour_rows(
        &mut group,
        &bush,
        &lines.way,
        -1.0,
        (top + CREST_BASIN.0 + 0.9, ARENA),
        &CONTOUR_LEAVES,
        &|p| {
            let zone = zones(p, &lines);
            zone.bank_land > 0.5
                && zone.bank_fall > BANK_COVER_DOWN + 0.04
                && zone.hillside < 0.5
                && off_lots(p, BANK_HEDGE_OUT + BANK_HEDGE.0 + 0.45)
                && in_arena(p)
                && open(p)
        },
    );
    let across = Street::SendaParkway.half() + PARKWAY + SIDEWALK + 1.0;
    contour_rows(
        &mut group,
        &bush,
        &lines.way,
        1.0,
        (across, ARENA),
        &CONTOUR_LEAVES,
        &|p| in_arena(p) && open(p),
    );
    // A shrub in each of the bank's crest basins.
    for (k, (middle, down)) in crest_basin_sites(&lines).into_iter().enumerate() {
        let at = middle + down * (CREST_BASIN.0 * 0.4);
        if open(at) {
            shrub(
                &mut group,
                &bush,
                at,
                0.55,
                0.7,
                2,
                &PLANTED_LEAVES,
                k as f64 * 5.3,
            );
        }
    }
    // A low shrub on Neblina's splitter island, toward its round end.
    let neblina = Street::NeblinaGaviota.line();
    let (from, _) = senda_layout::splitter_island();
    let (island, _) = neblina.at(from + SPLITTER_ISLAND.2);
    shrub(
        &mut group,
        &lump,
        island,
        0.35,
        0.8,
        2,
        &PLANTED_LEAVES,
        41.0,
    );
    let everywhere = (DVec2::splat(-SHRUB_EXTENT), DVec2::splat(SHRUB_EXTENT));
    // Drifts of shrubs along the hillside's trails, as the bank is planted.
    let hill_trails: Vec<Line> = hillside_trails().into_iter().map(Line).collect();
    let trail = |p: DVec2| {
        hill_trails
            .iter()
            .map(|trail| trail.distance(p))
            .fold(f64::INFINITY, f64::min)
    };
    clumps(&mut group, &bush, &PLANTED_CLUMPS, everywhere, 0.0, &|p| {
        let zone = zones(p, &lines);
        let drift = (p.x * 0.29 - p.y * 0.13).sin() + (p.y * 0.23 + p.x * 0.05 + 0.4).cos();
        let near = trail(p);
        near > TRAIL_SHRUBS.0
            && near < TRAIL_SHRUBS.1
            && drift > PLANTED_DRIFT
            && zone.hillside >= 0.5
            && off_lots(p, 0.8)
            && open(p)
    });
    // The hillside's coastal sage inside the arena, in its patches but for its trails.
    let hillside = (
        DVec2::new(HILLSIDE_FROM, -ARENA),
        DVec2::new(ARENA + 3.0, ARENA),
    );
    let sage = Clumps {
        spacing: HILL_SAGE_SPACING,
        ..SAGE_CLUMPS
    };
    clumps(&mut group, &bush, &sage, hillside, 0.0, &|p| {
        zones(p, &lines).hillside >= 0.5
            && hill_patch(p) > 0.5
            && trail(p) > 1.2
            && off_lots(p, 0.8)
            && open(p)
    });
    // The corner beds: a few metres round the corners where Rumbo leaves Neblina and
    // at the greenbelt's mouth.
    let kerb = STREET_HALF_WALK + SHRUB_PAVING_ROOM;
    // Rumbo leaves Neblina's east side (the left of its line).
    let corner = |p: DVec2| {
        let (neblina, rumbo) = (lines.gaviota.signed_distance(p), lines.rumbo.distance(p));
        neblina > 0.0 && neblina < kerb + CORNER_BED && rumbo < kerb + CORNER_BED
    };
    // Neblina's west corner with Senda Parkway: a berm of grasses before the first house.
    let way_corner = |p: DVec2| {
        let neblina = lines.gaviota.signed_distance(p);
        neblina < 0.0
            && -neblina < kerb + CORNER_BED
            && lines.way.distance(p) < way_reach + CORNER_BED
    };
    let near = (DVec2::splat(-ARENA), DVec2::splat(ARENA));
    clumps(&mut group, &lump, &GRASS_CLUMPS, near, 0.0, &|p| {
        (corner(p) || way_corner(p)) && off_lots(p, 0.3) && open(p) && !by_a_house(p, &houses)
    });
    // Down Senda Parkway's raised median and its south parkway.
    let trees = way_tree_sites();
    way_planting(&mut group, &lump, &bush, &lines, &trees);
    // The bend park's beds round its lawn and walk: ornamental grasses and agaves.
    if let Some(park) = bend_park(&lines) {
        let low = park
            .corners
            .iter()
            .copied()
            .fold(DVec2::splat(f64::INFINITY), DVec2::min);
        let high = park
            .corners
            .iter()
            .copied()
            .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
        clumps(&mut group, &lump, &GRASS_CLUMPS, (low, high), 0.0, &|p| {
            in_park_beds(&park, p) && open(p)
        });
    }
    // The entry beds: mounded shrubs of three sizes, a red-flowering one and an agave.
    for (k, bed) in entry_beds(&houses).into_iter().enumerate() {
        let (length, deep) = ENTRY_BED;
        for (n, &(u, v, plant)) in ENTRY_PLANTS.iter().enumerate() {
            let at = bed.middle - bed.toward_walk * (u * length) + bed.out * (v * deep);
            let seed = (k * 7 + n) as f64;
            match plant {
                EntryPlant::Shrub(size) => {
                    let tone = ENTRY_LEAVES[(k + n) % ENTRY_LEAVES.len()];
                    let size = size * (0.9 + 0.2 * hash(seed));
                    leafy(&mut group, &bush, (at, 0.0), (size, 0.8), tone, seed);
                }
                EntryPlant::Flowering => {
                    leafy(
                        &mut group,
                        &bush,
                        (at, 0.0),
                        (0.24, 0.8),
                        FLOWERING_LEAVES[1],
                        seed,
                    );
                }
                EntryPlant::Agave => agave(&mut group, &lump, at, 0.32, hash(seed) * 6.0),
            }
        }
    }
    front_bed_shrubs(&mut group, (&bush, &ball), &lines.lots);
    agaves(&mut group, &lump, &|p| {
        open(p) && off_lots(p, 0.5) && !by_a_house(p, &houses)
    });
    // A few red aloes on the corner wedge where Rumbo leaves Neblina.
    for (n, &(along, out)) in CORNER_ALOES.iter().enumerate() {
        let (point, direction) = lines.rumbo.at(STREET_HALF_WALK + along);
        let at = point + left(direction) * (STREET_HALF_WALK + out);
        if open(at) && off_lots(at, 0.4) && !by_a_house(at, &houses) {
            aloe(&mut group, &lump, at, n as f64 * 2.7);
        }
    }
    // A few agaves among the grasses on the corner wedge where Rumbo leaves Neblina.
    for (n, &(along, out)) in CORNER_AGAVES.iter().enumerate() {
        let (point, direction) = lines.rumbo.at(STREET_HALF_WALK + along);
        let at = point + left(direction) * (STREET_HALF_WALK + out);
        if open(at) && off_lots(at, 0.4) && !by_a_house(at, &houses) {
            agave(
                &mut group,
                &lump,
                at,
                0.38 + 0.08 * hash(n as f64),
                n as f64 * 1.3,
            );
        }
    }
    batch_in_cells(&mut group, 40.0);
    group
}

/// A plant in a house's entry bed.
#[derive(Clone, Copy)]
enum EntryPlant {
    /// A mounded shrub of this radius (m).
    Shrub(f64),
    Flowering,
    Agave,
}

/// One clipped boxwood ball at `at`.
fn boxwood(group: &mut Node, bush: &Arc<Mesh>, at: DVec2, seed: f64) {
    let (least, most) = BOXWOOD;
    let r = least + (most - least) * hash(seed);
    let mut ball = shadowed(
        bush.clone(),
        paint(BOXWOOD_LEAVES[(hash(seed + 0.5) * 2.0) as usize % 2]),
    );
    ball.scale = glam::DVec3::new(r, r * 0.85, r);
    ball.set_rotation_euler(0.0, hash(seed + 1.1) * 3.0, 0.0);
    put(group, ball, at.x, r * 0.8, at.y);
}

/// The low shrubs in the dark bed along each house's front, either side of its door, off
/// the garage's driveway.
fn front_bed_shrubs(group: &mut Node, (bush, ball): (&Arc<Mesh>, &Arc<Mesh>), lots: &[Lot]) {
    for (k, lot) in lots.iter().enumerate() {
        let design = lot.house.design();
        let (centre, _, along, _) = wall_frames(design.w, design.d)[design.door_wall];
        let plan = design.wall_plan(design.door_wall);
        let (Some(door), Some(garage)) = (plan.door(), plan.garage()) else {
            continue;
        };
        let in_lot = |u: f64| {
            let at = design.to_world(centre + along * u);
            lot.local(DVec2::new(lot.house.x + at.x, lot.house.z + at.z))
                .0
        };
        let (door, garage) = (in_lot(door), in_lot(garage));
        let side = (door - garage).signum();
        let spots = [
            door - side * (FRONT_WALK / 2.0 + 0.35),
            door + side * (FRONT_WALK / 2.0 + 0.4),
            door + side * (FRONT_WALK / 2.0 + 1.1),
        ];
        for (n, u) in spots.into_iter().enumerate() {
            if u.abs() > lot.half_width - 0.2 || (u - garage).abs() < GARAGE_BED_ROOM {
                continue;
            }
            let at = lot.at(u, lot.half_depth + FRONT_BED * 0.5);
            let seed = (k * 5 + n) as f64 * 2.3;
            shrub(
                group,
                bush,
                at,
                0.3 + 0.1 * hash(seed),
                0.7,
                2,
                &PLANTED_LEAVES,
                seed,
            );
        }
        // Clipped boxwood along the house's front, clear of the walk and the driveway.
        let mut u = -lot.half_width + 0.3;
        while u < lot.half_width - 0.25 {
            if (u - door).abs() > FRONT_WALK / 2.0 + 0.2 && (u - garage).abs() > GARAGE_BED_ROOM {
                let at = lot.at(u, lot.half_depth + BOXWOOD.1 + 0.08);
                boxwood(group, ball, at, k as f64 * 7.7 + u * 2.3);
            }
            u += BOXWOOD_STEP;
        }
        // Ornamental grass along the rest of the bed, low flowering shrubs among it,
        // clear of the front walk, the driveway and the shrubs by the door.
        let mut u = -lot.half_width + 0.35;
        let mut n = 0usize;
        while u < lot.half_width - 0.3 {
            let seed = (k * 37 + n) as f64 * 1.9;
            let clear = (u - door).abs() > FRONT_WALK / 2.0 + 0.25
                && (u - garage).abs() > GARAGE_BED_ROOM
                && spots.iter().all(|spot| (u - spot).abs() > 0.45);
            if clear {
                let at = lot.at(u, lot.half_depth + FRONT_BED * (0.6 + 0.25 * hash(seed)));
                if !n.is_multiple_of(3) {
                    shrub(group, bush, at, 0.28, 0.75, 2, &FLOWERING_LEAVES, seed);
                } else {
                    let size = 0.35 + 0.25 * hash(seed + 0.5);
                    tuft(group, at, size * 0.6, 1.4, &GRASS_LEAVES, seed);
                }
            }
            u += FRONT_BED_GRASS;
            n += 1;
        }
    }
}

/// What Senda Parkway's median and south parkway are planted with along their length,
/// one site every so far (m): a share of the sites a tuft of ornamental grass, then an
/// agave, then a red-flowering shrub, the rest bare blue-grey groundcover.
const WAY_PLANTING_STEP: f64 = 1.1;
const WAY_GRASS_SHARE: f64 = 0.45;
const WAY_AGAVE_SHARE: f64 = 0.62;
const WAY_FLOWER_SHARE: f64 = 0.74;
/// The tufts' size (m), knee high.
const WAY_GRASS: (f64, f64) = (0.22, 0.34);
/// A flowering shrub's leaves, a red-flowering lump among two green.
const FLOWERING_LEAVES: [u32; 3] = [0x55613e, 0x8a3f3a, 0x4d5a3a];

/// Senda Parkway's planting, scenery a tank pushes through: down the raised median between
/// its trees and its openings, and down the south parkway between its kerb and its path,
/// broken round its trees and lamps and where Neblina meets it; tufts of straw-coloured
/// grass, blue-grey agaves and red-flowering shrubs over the groundcover, not a hedge.
fn way_planting(
    group: &mut Node,
    lump: &Arc<Mesh>,
    bush: &Arc<Mesh>,
    lines: &Lines,
    trees: &[DVec2],
) {
    let half = Street::SendaParkway.half();
    let lamps = way_lamp_sites();
    let neblina = Street::NeblinaGaviota.line();
    let parkway_skip = |p: DVec2| {
        trees.iter().any(|tree| tree.distance(p) < 1.0)
            || lamps.iter().any(|lamp| lamp.distance(p) < 0.5)
            || neblina.distance(p) < STREET_HALF_WALK + 0.3
    };
    let median_skip =
        |p: DVec2| trees.iter().any(|tree| tree.distance(p) < 1.2) || in_median_opening(p, 0.3);
    let mut median = Node::default();
    median.position.y = KERB_HEIGHT;
    let mut parkway = Node::default();
    // The median's middle (either side of it, staggered) and the south parkway's.
    let rows: [(f64, f64, bool); 3] = [
        (MEDIAN_HALF * 0.4, 0.0, true),
        (-MEDIAN_HALF * 0.4, WAY_PLANTING_STEP / 2.0, true),
        (-(half + WAY_PARKWAY / 2.0), 0.3, false),
    ];
    for (k, (offset, start, in_median)) in rows.into_iter().enumerate() {
        let mut s = start;
        while s < lines.way.length() {
            let (point, along) = lines.way.at(s);
            let seed = s * 1.73 + k as f64 * 31.0;
            let at = point
                + left(along) * (offset + (hash(seed) - 0.5) * 0.3)
                + along * (hash(seed + 0.4) - 0.5) * 0.4;
            s += WAY_PLANTING_STEP;
            let skip = if in_median {
                median_skip(at)
            } else {
                parkway_skip(at)
            };
            if skip || !in_arena_or_near(at) {
                continue;
            }
            let target = if in_median { &mut median } else { &mut parkway };
            let pick = hash(seed + 1.3);
            if pick < WAY_GRASS_SHARE {
                let size = WAY_GRASS.0 + (WAY_GRASS.1 - WAY_GRASS.0) * hash(seed + 2.1);
                tuft(target, at, size, 1.3, &GRASS_LEAVES, seed);
            } else if pick < WAY_AGAVE_SHARE {
                agave(
                    target,
                    lump,
                    at,
                    0.3 + 0.15 * hash(seed + 2.6),
                    hash(seed + 2.9) * 6.0,
                );
            } else if pick < WAY_FLOWER_SHARE {
                shrub(
                    target,
                    bush,
                    at,
                    0.3 + 0.08 * hash(seed + 3.3),
                    0.7,
                    3,
                    &FLOWERING_LEAVES,
                    seed,
                );
            }
        }
    }
    // Merged here: the outer batch leaves nested groups alone.
    batch_in_cells(&mut median, 40.0);
    batch_in_cells(&mut parkway, 40.0);
    group.children.push(median);
    group.children.push(parkway);
}

/// Whether `p` lies within a few metres of the arena, where the Way's hedge still shows.
fn in_arena_or_near(p: DVec2) -> bool {
    p.x.abs().max(p.y.abs()) < ARENA + 20.0
}

/// A cluster of agaves at each cul-de-sac's end on the hillside, a little to one side
/// of its down-drain, wherever `wanted` holds: blue-grey rosettes of stiff leaves.
fn agaves(group: &mut Node, lump: &Arc<Mesh>, wanted: &dyn Fn(DVec2) -> bool) {
    for (k, (start, out)) in cul_de_sac_ends().into_iter().enumerate() {
        let middle = start + out * (CUL_DE_SAC_END * 0.5) + left(out) * 2.2;
        for n in 0..AGAVES {
            let seed = (k * 31 + n) as f64;
            let angle = hash(seed) * std::f64::consts::TAU;
            let at = middle + DVec2::new(angle.cos(), angle.sin()) * (0.3 + 1.4 * hash(seed + 0.5));
            if !wanted(at) {
                continue;
            }
            agave(group, lump, at, 0.35 + 0.2 * hash(seed + 1.5), angle);
        }
    }
}

/// One agave `size` across at `at`, turned `angle`: a blue-grey rosette of seven stiff
/// leaves round its heart.
fn agave(group: &mut Node, lump: &Arc<Mesh>, at: DVec2, size: f64, angle: f64) {
    agave_of(group, lump, (at, 0.0), (size, angle), AGAVE);
}

/// One rosette like [`agave`]'s standing on ground `base` metres up, in `leaves`.
fn agave_of(
    group: &mut Node,
    lump: &Arc<Mesh>,
    (at, base): (DVec2, f64),
    (size, angle): (f64, f64),
    leaves: u32,
) {
    let mut heart = shadowed(lump.clone(), paint(leaves));
    heart.scale = glam::DVec3::new(size * 0.3, size * 0.45, size * 0.3);
    put(group, heart, at.x, base + size * 0.2, at.y);
    // Broad fleshy leaves, widest a third of the way out and pointed, an outer ring
    // splayed low and an inner one turned up.
    for &(count, tilt, length, phase) in &AGAVE_RINGS {
        for leaf in 0..count {
            let turn = (leaf as f64 + phase) / count as f64 * std::f64::consts::TAU + angle;
            let half = size * length / 2.0;
            let out = DVec2::new(turn.sin(), turn.cos()) * (half * tilt.cos() * 0.9);
            let mut blade = shadowed(lump.clone(), paint(leaves));
            blade.scale = glam::DVec3::new(size * 0.17, size * 0.045, half);
            blade.rotation = DQuat::from_rotation_y(turn) * DQuat::from_rotation_x(-tilt);
            let y = base + 0.06 + half * tilt.sin();
            put(group, blade, at.x + out.x, y, at.y + out.y);
        }
    }
}

/// A bank of cluster mailboxes round `middle`, their doors facing `out`: a row of
/// bronze-green cabinets on pedestals on a concrete pad.
fn mailbox_bank(group: &mut Node, middle: DVec2, out: DVec2) {
    let direction = -left(out);
    let (w, h, d) = MAILBOX;
    let yaw = out.x.atan2(out.y);
    let span = w * MAILBOX_UNITS as f64 + 0.3;
    put(
        group,
        rotated(
            box_part(span, 0.1, d + 0.4, MAILBOX_PAD, 0.0),
            0.0,
            yaw,
            0.0,
        ),
        middle.x,
        0.05,
        middle.y,
    );
    for k in 0..MAILBOX_UNITS {
        let u = (k as f64 - (MAILBOX_UNITS as f64 - 1.0) / 2.0) * (w + 0.04);
        let at = middle + direction * u;
        put(
            group,
            rotated(
                box_part(w * 0.4, MAILBOX_PEDESTAL, d * 0.4, MAILBOX_PAINT, 0.0),
                0.0,
                yaw,
                0.0,
            ),
            at.x,
            0.1 + MAILBOX_PEDESTAL / 2.0,
            at.y,
        );
        put(
            group,
            rotated(box_part(w, h, d, MAILBOX_PAINT, 0.0), 0.0, yaw, 0.0),
            at.x,
            0.1 + MAILBOX_PEDESTAL + h / 2.0,
            at.y,
        );
    }
}

/// A house's entry bed: its middle, the way its front faces (out to the street), the
/// way along its front toward the front walk, and the house's stucco.
struct EntryBed {
    middle: DVec2,
    out: DVec2,
    toward_walk: DVec2,
    stucco: u32,
    /// Whether the house's porch is brick, and the seat wall's cap with it.
    brick: bool,
}

/// Where each house's entry bed lies: beside the front walk on the side away from the
/// garage, at the street end of the front yard, clear of the porch.
fn entry_beds(houses: &[SendaHouse]) -> Vec<EntryBed> {
    houses
        .iter()
        .filter_map(|house| {
            let design = house.design();
            let (centre, out, along, _) = wall_frames(design.w, design.d)[design.door_wall];
            let plan = design.wall_plan(design.door_wall);
            let (door, garage) = (plan.door()?, plan.garage()?);
            let away = -(garage - door).signum();
            let u = door + away * (FRONT_WALK / 2.0 + 0.2 + ENTRY_BED.0 / 2.0);
            let reach = house.setback - ENTRY_BED.1 / 2.0 - 0.15;
            let at = design.to_world(centre + along * u + out * reach);
            let (out, along) = (design.to_world(out), design.to_world(along));
            Some(EntryBed {
                middle: DVec2::new(house.x + at.x, house.z + at.z),
                out: DVec2::new(out.x, out.z),
                toward_walk: DVec2::new(along.x, along.z) * -away,
                stucco: design.style.siding,
                brick: design.style.arched_porch,
            })
        })
        .collect()
}

/// Each house's entry bed: bark mulch behind a low seat wall in the house's own stucco
/// along its street side, turning up beside the front walk toward the porch, under a
/// pale precast cap, with a capped pilaster at each end and at the corner (its plants
/// are with the other shrubs).
fn entry_bed_beds(group: &mut Node, houses: &[SendaHouse]) {
    for bed in entry_beds(houses) {
        let yaw = bed.out.x.atan2(bed.out.y);
        let (length, deep) = ENTRY_BED;
        let mut piece = |node: Node, at: DVec2, y: f64| {
            put(group, rotated(node, 0.0, yaw, 0.0), at.x, y, at.y);
        };
        piece(
            box_part(length, SLAB, deep, BED_MULCH, 0.0),
            bed.middle,
            0.055 - SLAB / 2.0,
        );
        let (height, thick) = SEAT_WALL;
        let (cap, stone) = SEAT_CAP;
        let cap_paint = if bed.brick { BRICK_CAP } else { stone };
        let street = bed.middle + bed.out * (deep / 2.0 - thick / 2.0);
        let walk = bed.middle + bed.toward_walk * (length / 2.0 - thick / 2.0);
        for (at, w, d) in [(street, length, thick), (walk, thick, deep)] {
            piece(
                box_part(w, height - cap, d, bed.stucco, 0.0),
                at,
                (height - cap) / 2.0,
            );
            piece(
                box_part(w + 0.04, cap, d + 0.04, cap_paint, 0.0),
                at,
                height - cap / 2.0,
            );
        }
        let (side, tall) = SEAT_PILASTER;
        let (along, out) = (length / 2.0 - side / 2.0, deep / 2.0 - side / 2.0);
        for (u, v) in [(-along, out), (along, out), (along, -out)] {
            let at = bed.middle + bed.toward_walk * u + bed.out * v;
            piece(
                box_part(side, tall - cap, side, bed.stucco, 0.0),
                at,
                (tall - cap) / 2.0,
            );
            piece(
                box_part(side + 0.06, cap, side + 0.06, cap_paint, 0.0),
                at,
                tall - cap / 2.0,
            );
        }
    }
}

/// An acorn lamp `height` tall with a globe of radius `globe`, at `(x, z)`.
fn lamp(group: &mut Node, globe_mesh: &Arc<Mesh>, (x, z): (f64, f64), height: f64, globe: f64) {
    put(
        group,
        cylinder_part(LAMP_POLE * height / LAMP_HEIGHT, height, LAMP_METAL, 6),
        x,
        height / 2.0,
        z,
    );
    let mut glass = shadowed(globe_mesh.clone(), paint(LAMP_GLASS));
    glass.scale = glam::DVec3::new(globe, globe * 1.3, globe);
    put(group, glass, x, height + globe, z);
    put(
        group,
        cylinder_part(globe * 0.8, 0.07, LAMP_METAL, 8),
        x,
        height + globe * 2.3,
        z,
    );
}

/// The acorn street lamps along the parkways, by the side yard beside every few houses,
/// clear of the asphalt, and Senda Parkway's tall ones along both its parkways between its
/// trees. Scenery, not cover: a tank brushes past a pole.
fn street_lamps() -> Node {
    let mut group = Node::group("senda-lamps");
    let globe = Arc::new(sphere_geometry(1.0, 8, 6));
    let lines: Vec<(Street, Line)> = Street::ALL.map(|street| (street, street.line())).into();
    for house in senda_houses().iter().step_by(LAMP_EVERY) {
        let design = house.design();
        let (centre, out, along, half) = wall_frames(design.w, design.d)[design.door_wall];
        let reach = house.setback + SIDEWALK + PARKWAY / 2.0;
        let spot = design.to_world(centre + along * (half + SIDE_YARD / 2.0) + out * reach);
        let (x, z) = (house.x + spot.x, house.z + spot.z);
        let on_asphalt = lines
            .iter()
            .any(|(street, line)| line.distance(DVec2::new(x, z)) < street.half() + 0.2);
        if on_asphalt {
            continue;
        }
        lamp(&mut group, &globe, (x, z), LAMP_HEIGHT, LAMP_GLOBE);
    }
    for at in way_lamp_sites() {
        lamp(
            &mut group,
            &globe,
            (at.x, at.y),
            WAY_LAMP_HEIGHT,
            WAY_LAMP_GLOBE,
        );
    }
    batch(&mut group);
    group
}

/// Where Senda Parkway's tall lamps stand, in the middle of both its parkways, short of the
/// level's wall where the Way meets it.
fn way_lamp_sites() -> Vec<DVec2> {
    let way = Street::SendaParkway.line();
    let half = Street::SendaParkway.half();
    let boundary = senda_boundary();
    let mut sites = Vec::new();
    let mut s = MEDIAN_TREE_SPACING / 4.0;
    while s < way.length() {
        let (point, along) = way.at(s);
        for (side, strip) in [(1.0, PARKWAY), (-1.0, WAY_PARKWAY)] {
            let site = point + left(along) * side * (half + strip / 2.0);
            if clear_of_wall(&boundary, site, WAY_WALL_ROOM) {
                sites.push(site);
            }
        }
        s += WAY_LAMP_SPACING;
    }
    sites
}

/// A lot wall's precast cap: its height (m), how far it overhangs each face, and its
/// paint, a little paler than the block; the square block piers either side of a gate
/// (side, m, and how far they rise over the wall), and how far the gate's boards
/// (thickness, m) stand under the wall's top.
const YARD_WALL_CAP: (f64, f64, u32) = (0.07, 0.04, 0xb19c82);
/// The block's coursing: a shadowed bed joint (its height, m, how far it stands off the
/// face, and its paint) under every course of this height (m), and a head joint this wide
/// (m) at the end of every block of this length (m), half a block on in every other course
/// (running bond).
const BLOCK_JOINT: (f64, f64, u32) = (0.014, 0.004, 0x6a5848);
/// A block wall's paints: its split face, its cap and its joints.
#[derive(Clone, Copy)]
struct BlockLook {
    face: u32,
    cap: u32,
    joint: u32,
}
/// The lots' brown-tan block, and the tract's dark brown perimeter block behind Neblina's
/// west row.
const LOT_BLOCK: BlockLook = BlockLook {
    face: YARD_WALL_COLOR,
    cap: YARD_WALL_CAP.2,
    joint: BLOCK_JOINT.2,
};
const PERIMETER_BLOCK: BlockLook = BlockLook {
    face: YARD_PERIMETER_COLOR,
    cap: 0x7a6450,
    joint: 0x3f3229,
};
const BLOCK_COURSE: f64 = 0.2;
const HEAD_JOINT: f64 = 0.014;
const BLOCK_LENGTH: f64 = 0.4;
/// The split face: the weathered concrete photo, whose mottle reads as the block's rough
/// face, tinted from its average sRGB colour to the block's, a tile this many metres
/// across, and how deep its bump shades.
const BLOCK_TILE: f64 = 1.4;
const BLOCK_TEXTURE_AVERAGE: u32 = 0xa9a7a1;
const BLOCK_BUMP: f32 = 0.06;
/// The pilaster at the start of each run, where the lot lines meet: its side (m), how
/// far it stands proud of each face and over the wall (m).
const PILASTER: (f64, f64, f64) = (0.36, 0.1, 0.1);
const GATE_PIER: (f64, f64) = (0.3, 0.12);
const GATE_BOARDS: (f64, f64) = (0.05, 0.12);
/// A view fence's block base (its height, m) under the steel, and its rails' side and
/// how far (m) they run from the base's cap and the fence's top.
const VIEW_BASE: f64 = 0.45;
const VIEW_RAIL: (f64, f64) = (0.04, 0.08);

/// Whether a concrete cover of `color` is one of Senda's lot walls, view fences or
/// gates.
/// A loggia or pergola over a lot's patio, as its cover draws it: in its box's own frame,
/// `w` across by `d` with +z toward the house's back wall and the ground at 0. A loggia is
/// a shed roof in its house's tile, falling from the wall at the eave line to a stucco
/// beam on square stucco columns, the tile's courses across it; a pergola a beam over its
/// posts and one along the house, slats across.
pub fn patio_cover(group: &mut Node, look: PatioCover, (w, d): (f64, f64), style: u8) {
    let (low, eave) = PATIO_ROOF;
    let mut part = |node: Node, x: f64, y: f64, z: f64| put(group, node, x, y, z);
    match look {
        PatioCover::Loggia => {
            let style = SENDA_STYLES[usize::from(style) % SENDA_STYLES.len()];
            let (column, beam) = LOGGIA_COLUMN;
            let cover = w - 2.0 * LOGGIA_EAVE;
            let reach = d - LOGGIA_EAVE;
            let front = d / 2.0 - reach + column / 2.0;
            for end in [-1.0, 1.0] {
                part(
                    box_part(column, low, column, style.siding, 0.0),
                    end * (cover - PATIO_POST) / 2.0,
                    low / 2.0,
                    front,
                );
            }
            part(
                box_part(cover + 0.1, beam, column, style.siding, 0.0),
                0.0,
                low - beam / 2.0 + 0.02,
                front,
            );
            let pitch = (eave - low).atan2(reach);
            let slope = reach.hypot(eave - low) + LOGGIA_EAVE;
            let mut roof = Node::default();
            put(
                &mut roof,
                box_part(w, 0.09, slope, style.roof, 0.0),
                0.0,
                0.0,
                0.0,
            );
            // The tile's courses: a shadowed butt every so far down the slope.
            let (course, butt) = LOGGIA_TILE_COURSE;
            let mut along_slope = -slope / 2.0 + course;
            while along_slope < slope / 2.0 - 0.05 {
                put(
                    &mut roof,
                    box_part(w, butt, butt, darker(style.roof, LOGGIA_BUTT_SHADE), 0.0),
                    0.0,
                    0.045 + butt / 2.0,
                    along_slope,
                );
                along_slope += course;
            }
            roof.rotation = DQuat::from_rotation_x(-pitch);
            put(group, roof, 0.0, (low + eave) / 2.0 + 0.08, 0.0);
        }
        PatioCover::Pergola => {
            let cover = w - 0.1;
            let reach = d - 0.1;
            for end in [-1.0, 1.0] {
                part(
                    box_part(PATIO_POST, low, PATIO_POST, PERGOLA_WOOD, 0.0),
                    end * (cover - PATIO_POST) / 2.0,
                    low / 2.0,
                    d / 2.0 - reach + PATIO_POST / 2.0,
                );
            }
            for z in [d / 2.0 - reach + PATIO_POST / 2.0, d / 2.0 - 0.06] {
                part(
                    box_part(cover + 0.1, 0.12, 0.1, PERGOLA_WOOD, 0.0),
                    0.0,
                    low + 0.06,
                    z,
                );
            }
            let (slat, spacing) = PERGOLA_SLAT;
            let count = (cover / spacing).floor() as usize;
            for n in 0..=count {
                let x = (n as f64 / count.max(1) as f64 - 0.5) * (cover - slat);
                part(
                    box_part(slat, 0.06, reach + 0.2, PERGOLA_WOOD, 0.0),
                    x,
                    low + 0.15,
                    d / 2.0 - reach / 2.0,
                );
            }
        }
    }
}

pub fn is_yard_wall(color: u32) -> bool {
    color == YARD_WALL_COLOR
        || color == YARD_VIEW_COLOR
        || color == YARD_GATE_COLOR
        || color == YARD_PERIMETER_COLOR
}

/// A run of a lot's wall, a `w` × `d` × `h` box of `color` turned `turn` about +y with
/// its line: split-face block under its cap, a black steel view fence on a low block
/// base between block pilasters, or a gate return, stained boards between two capped
/// block piers. Each is built along x and turned a quarter more where the box runs along
/// z.
pub fn yard_wall(group: &mut Node, (w, d, h): (f64, f64, f64), color: u32, turn: f64) {
    group.rotation = DQuat::from_rotation_y(turn);
    let (length, thick) = if w >= d { (w, d) } else { (d, w) };
    let mut run = Node::default();
    if w < d {
        run.rotation = DQuat::from_rotation_y(FRAC_PI_2);
    }
    match color {
        YARD_GATE_COLOR => gate_return(&mut run, length, h),
        YARD_VIEW_COLOR => view_fence_run(&mut run, length, thick, h),
        YARD_PERIMETER_COLOR => block_run(&mut run, length, thick, h, PERIMETER_BLOCK),
        _ => block_run(&mut run, length, thick, h, LOT_BLOCK),
    }
    group.children.push(run);
}

/// A part centred `u` along a run and `y` up.
fn along(run: &mut Node, part: Node, u: f64, y: f64) {
    put(run, part, u, y, 0.0);
}

/// A box of split-face block `length` along x, `height` tall and `thick` across, its
/// face's texture laid at its true size on every side.
fn block_box(length: f64, height: f64, thick: f64, look: BlockLook) -> Node {
    let mut geometry = box_geometry(length, height, thick);
    for i in 0..geometry.positions.len() {
        let [x, y, z] = geometry.positions[i].map(f64::from);
        let [nx, ny, nz] = geometry.normals[i].map(|n| f64::from(n).abs());
        let u = if nx > ny && nx > nz { z } else { x };
        let v = if ny >= nx && ny >= nz { z } else { y };
        geometry.uvs[i] = [(u / BLOCK_TILE) as f32, (v / BLOCK_TILE) as f32];
    }
    let face = textured(
        CONCRETE_TEXTURE,
        tint_over(look.face, BLOCK_TEXTURE_AVERAGE),
        BLOCK_BUMP,
        0.95,
    );
    shadowed(Arc::new(geometry), face)
}

/// A block wall's cap `length` long over a wall `thick` across, its top at `top`.
fn wall_cap(run: &mut Node, length: f64, thick: f64, top: f64, look: BlockLook) {
    let (cap, overhang, _) = YARD_WALL_CAP;
    let paint_of = look.cap;
    along(
        run,
        box_part(length, cap, thick + 2.0 * overhang, paint_of, 0.0),
        0.0,
        top - cap / 2.0,
    );
}

/// A capped block pilaster `side` along the run and `deep` across it at `u`, its cap's
/// top at `top`.
fn pilaster(run: &mut Node, (side, deep): (f64, f64), u: f64, top: f64, look: BlockLook) {
    let (cap, overhang, _) = YARD_WALL_CAP;
    let paint_of = look.cap;
    let body = top - cap;
    along(run, block_box(side, body, deep, look), u, body / 2.0);
    along(
        run,
        box_part(
            side + 2.0 * overhang,
            cap,
            deep + 2.0 * overhang,
            paint_of,
            0.0,
        ),
        u,
        top - cap / 2.0,
    );
}

/// The block's coursing over a wall `length` long and `thick` across, from the ground to
/// `top`: a bed joint under each course across both faces, and the head joints between
/// its blocks in running bond, flat strips on both faces in one mesh.
fn coursing(run: &mut Node, length: f64, thick: f64, top: f64, look: BlockLook) {
    let (joint, proud, _) = BLOCK_JOINT;
    let paint_of = look.joint;
    let mut y = BLOCK_COURSE;
    while y < top - 0.05 {
        along(
            run,
            box_part(length, joint, thick + 2.0 * proud, paint_of, 0.0),
            0.0,
            y,
        );
        y += BLOCK_COURSE;
    }
    let mut positions: Vec<f64> = Vec::new();
    let mut normals: Vec<f64> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut course = 0usize;
    let mut y0 = 0.0;
    while y0 < top - 0.05 {
        let y1 = (y0 + BLOCK_COURSE).min(top);
        let shift = if course.is_multiple_of(2) {
            BLOCK_LENGTH
        } else {
            BLOCK_LENGTH / 2.0
        };
        let mut u = -length / 2.0 + shift;
        while u < length / 2.0 - 0.05 {
            let (x0, x1) = (u - HEAD_JOINT / 2.0, u + HEAD_JOINT / 2.0);
            for face in [-1.0, 1.0] {
                let z = face * (thick / 2.0 + proud);
                // Wound counter-clockwise seen from outside the face.
                let corners = if face > 0.0 {
                    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                } else {
                    [(x1, y0), (x0, y0), (x0, y1), (x1, y1)]
                };
                let first = (positions.len() / 3) as u32;
                for (x, y) in corners {
                    positions.extend([x, y, z]);
                    normals.extend([0.0, 0.0, face]);
                }
                indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
            }
            u += BLOCK_LENGTH;
        }
        y0 += BLOCK_COURSE;
        course += 1;
    }
    if positions.is_empty() {
        return;
    }
    let uvs = vec![0.0; positions.len() / 3 * 2];
    run.children.push(shadow_receiver(
        Arc::new(Mesh::from_f64(&positions, &normals, &uvs, Some(indices))),
        paint(paint_of),
    ));
}

/// A run of split-face block under its cap, with a capped pilaster at its start, so one
/// stands at every joint between runs and at the lot lines' corners.
fn block_run(run: &mut Node, length: f64, thick: f64, h: f64, look: BlockLook) {
    let cap = YARD_WALL_CAP.0;
    along(
        run,
        block_box(length, h - cap, thick, look),
        0.0,
        (h - cap) / 2.0,
    );
    wall_cap(run, length, thick, h, look);
    coursing(run, length, thick, h - cap, look);
    let (side, out, rise) = PILASTER;
    pilaster(
        run,
        (side, thick + 2.0 * out),
        -length / 2.0 + side / 2.0,
        h + rise,
        look,
    );
}

/// A run of tubular steel view fence: black pickets between a bottom and a top rail over
/// a low capped block base, between capped block pilasters at its start and, on a long
/// run, its middle.
fn view_fence_run(run: &mut Node, length: f64, thick: f64, h: f64) {
    let cap = YARD_WALL_CAP.0;
    along(
        run,
        block_box(length, VIEW_BASE - cap, thick, LOT_BLOCK),
        0.0,
        (VIEW_BASE - cap) / 2.0,
    );
    wall_cap(run, length, thick, VIEW_BASE, LOT_BLOCK);
    coursing(run, length, thick, VIEW_BASE - cap, LOT_BLOCK);
    let (side, out, rise) = PILASTER;
    let mut posts = vec![-length / 2.0 + side / 2.0];
    if length > VIEW_FENCE.2 {
        posts.push(side / 4.0);
    }
    for &u in &posts {
        pilaster(run, (side, thick + 2.0 * out), u, h + rise, LOT_BLOCK);
    }
    let (rail, inset) = VIEW_RAIL;
    for y in [VIEW_BASE + inset, h - inset] {
        along(run, box_part(length, rail, rail, FENCE_STEEL, 0.0), 0.0, y);
    }
    let (picket, spacing) = PICKETS;
    let tall = h - VIEW_BASE - 0.02;
    let mut u = -length / 2.0 + side + spacing / 2.0;
    while u < length / 2.0 - spacing / 3.0 {
        if posts
            .iter()
            .all(|&post| (u - post).abs() > side / 2.0 + 0.02)
        {
            along(
                run,
                box_part(picket, tall, picket, FENCE_STEEL, 0.0),
                u,
                VIEW_BASE + tall / 2.0,
            );
        }
        u += spacing;
    }
}

/// A gate return: stained boards between two capped block piers.
fn gate_return(run: &mut Node, length: f64, h: f64) {
    let (pier, rise) = GATE_PIER;
    for end in [-1.0, 1.0] {
        pilaster(
            run,
            (pier, pier),
            end * (length - pier) / 2.0,
            h + rise,
            LOT_BLOCK,
        );
    }
    let (boards, drop) = GATE_BOARDS;
    let span = (length - 2.0 * pier).max(0.1);
    let tall = h - drop - 0.05;
    along(
        run,
        box_part(span, tall, boards, YARD_GATE_COLOR, 0.0),
        0.0,
        0.05 + tall / 2.0,
    );
}
