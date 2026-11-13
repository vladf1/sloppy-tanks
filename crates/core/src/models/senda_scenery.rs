//! Senda's retained scenery: the ground the neighbourhood stands on (the lots' small
//! front lawns or gravel and their beds, the greenbelt's low groundcover, the groundcover
//! along Senda Parkway, the mulched and planted slopes of the bank below it, drying to
//! meadow at its east end, and round the tract, the park's lawn on the field behind
//! Neblina's west row, and the grey-green sage of the hillside, rising past the arena
//! into green rolling hills), Senda Parkway with its kerbed, planted median, bike lanes and
//! the wide path along its south side, Neblina, Rumbo and Gaviota paved along their
//! traced curves with kerbs, parkways and sidewalks, the cul-de-sacs and their planted
//! islands, each house's driveway, entry walk and the bed along its office, the walled
//! back yards (a patio, a planter strip of low shrubs along the back wall, on most lots
//! a tile-roofed rear loggia or a pergola, and at most one small prop), the greenbelt's
//! planted slope up to Rumbo with its mulch walk, the planted strip behind Neblina's west
//! row (rosemary and olives under the dark perimeter block) and the scrub behind
//! Gaviota's south row, the park's walks, plazas, benches and trees, the terrace walk and
//! spur walks on the bank and the drains round the cul-de-sacs' ends, the spawn pads,
//! and oaks and sage past the arena's east edge and in a belt past the south and west
//! walls. The lots' walls are cover
//! ([`crate::sim::senda_yards`]); [`yard_wall`] draws them.
//!
//! The paving is drawn, not collided: tanks drive on the arena's flat floor, and the
//! slabs stand a few centimetres proud of the grass as the village roads float.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::{DQuat, DVec2, DVec3};

use crate::geometry::{
    Mesh, RingGeometry, box_geometry, cylinder_geometry, icosahedron_geometry, octahedron_geometry,
    plane_geometry_segments, sphere_geometry,
};
use crate::scene::Node;
use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
use crate::sim::house_layout::SENDA_STYLES;
use crate::sim::kinds::{CoverKind, Team};
use crate::sim::model_data::ARENA;
use crate::sim::senda_layout::{
    self, CUL_DE_SAC_ISLAND, CUL_DE_SAC_RADIUS, DRIVEWAY_PAST_DOOR, HOUSE_DEPTH, HOUSE_WIDTH,
    LAMP_HEIGHT, Line, MEDIAN_HALF, PARKWAY, PASEO_WIDTH, Row, SENDA_PICKUPS, SENDA_SCALE,
    SIDE_YARD, SIDEWALK, SOUTH_LANE, SPLITTER_ISLAND, STREET_LAMP_COLORS, SendaHouse, Street,
    WAY_LAMP_HEIGHT, WAY_PARKWAY, WAY_PATH, greenbelt_mouth, house_front, in_arena,
    in_median_opening, inside_boundary, is_senda_paved, lamp_is_cover, lamp_sites, left,
    neblina_bend, oak_site, outer_way_tree_sites, paseos, senda_boundary, senda_houses,
    way_tree_sites,
};
use crate::sim::senda_yards::{
    LOGGIA_COLUMN, LOGGIA_EAVE, Lot, PATIO_POST, PATIO_ROOF, PERGOLA_WOOD, PatioCover,
    YARD_GATE_COLOR, YARD_PERIMETER_COLOR, YARD_PLANTER, YARD_VIEW_COLOR, YARD_WALL_COLOR,
    YARD_WALL_THICKNESS, back_yard, front_openings, greenbelt_lane, house_back, park_corners,
    patio_covers, patio_depth, porch, senda_lots, turf_side,
};
use crate::sim::tree_proportions::tree_proportions;

use super::batching::{batch, batch_in_cells, paint_mesh};
use super::building_kit::{textured, tint_over};
use super::concrete_surfaces::CONCRETE_TEXTURE;
use super::ground_surfaces::{GroundKind, ground_material, ground_uvs, repeated_ground_material};
use super::model_primitives::{
    box_part, cylinder_part, paint, put, rotated, shadow_receiver, shadowed,
};
use super::scenery::create_spawn_pads;
use super::senda_hills::Hills;
use super::senda_plants::{ShrubForm, rounded_shrub};
use super::tree_models::{TreeDetail, TreeShape, leafy_shrub, tree_model};

/// From a residential street's centre line to the back of its sidewalk (m).
const STREET_HALF_WALK: f64 = crate::sim::senda_layout::STREET_HALF + PARKWAY + SIDEWALK;
/// How far the grass reaches across, centred on the arena (m), past the hills' crests,
/// and its vertices' spacing (m).
const GROUND_EXTENT: f64 = 340.0;
const GROUND_STEP: f64 = 2.75;
/// Paving colours: asphalt weathered to the light grey of the tract's streets (about
/// two thirds as bright as the sidewalk), sidewalk and kerb concrete, driveways a warmer
/// broom-finished concrete, and Senda Parkway's raised median, bark mulch under its
/// shrubs and trees.
const ASPHALT: u32 = 0x707274;
const SIDEWALK_CONCRETE: u32 = 0xc4bfb3;
const KERB: u32 = 0xd3cec2;
/// The concrete gutter pan along every kerb's foot: its width (m) on the asphalt and its
/// colour, a little darker than the kerb, which gives each street its light edge from
/// above.
const GUTTER: (f64, u32) = (0.4, 0xb0aa9e);
/// The narrower pan round the turning circles' islands and along the splitter island.
const ISLAND_GUTTER: f64 = 0.25;
/// The walks' score lines: one every so far (m), their width (m) and colour.
const WALK_SCORE: f64 = 1.4;
const CONCRETE_JOINT_WIDTH: f64 = 0.025;
const CONCRETE_JOINT: u32 = 0x958f83;
/// The residential streets' paving seam (its offset from the middle and width, m) and
/// crack sealant; their utility trench patches (length along the street and width, m,
/// about one every so far, m), long and narrow beside a gutter, only a shade darker than
/// the asphalt round them so they never read as holes.
const ASPHALT_SEAM: (f64, f64) = (0.45, 0.05);
const ASPHALT_SEALED: u32 = 0x65676a;
const ASPHALT_PATCH: (f64, f64) = (4.4, 0.65);
const ASPHALT_PATCH_STEP: f64 = 19.0;
const ASPHALT_PATCH_TONE: u32 = 0x6a6c6e;
const DRIVEWAY: u32 = 0xbdb3a1;
/// Where the driveways' cross joints fall, as shares from the garage door to the
/// sidewalk.
const DRIVEWAY_JOINTS: [f64; 3] = [0.34, 0.67, 1.0];
const MEDIAN: u32 = 0x6e604e;
/// Slab thickness and kerb height and width (m).
const SLAB: f64 = 0.05;
const KERB_HEIGHT: f64 = 0.13;
const KERB_WIDTH: f64 = 0.16;
/// Paving heights over the floor: the streets, then the kerbs, sidewalks and median
/// (raised over the asphalt they cross so junctions do not flicker).
const STREET_TOP: f64 = 0.045;
/// What lies on a street's asphalt, each this far (m) over it and clear of the one
/// under it (laid a millimetre or two apart they flickered against each other from the
/// play camera): a turning circle's own asphalt over its street's end, the paving seam,
/// the patches, the gutter pans and the painted lines.
const CIRCLE_LIFT: f64 = 0.003;
const SEAM_LIFT: f64 = 0.006;
const PATCH_LIFT: f64 = 0.009;
const GUTTER_LIFT: f64 = 0.012;
const PAINT_LIFT: f64 = 0.016;
/// A joint scored in a slab or a sidewalk stands this far (m) over it.
const JOINT_LIFT: f64 = 0.004;
const WALK_TOP: f64 = 0.06;
/// The entry walk's width, and the strip of darker pavers bordering some driveways along
/// their outer side (width, paint).
const FRONT_WALK: f64 = 0.9;
const PAVER_STRIP: (f64, u32) = (0.3, 0x9a8470);
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
/// The park's mown lawn on the west field, over the packed-dirt tile as the front lawns
/// are (the planted ground draws it): calmer and greener than the natural ground round
/// it, a little drier than the front lawns' [`FRONT_LAWN_TINT`].
const WEST_PARK_LAWN: [f32; 3] = [0.22, 0.56, 0.84];
const MULCH: [f32; 3] = [0.46, 0.44, 0.9];
/// The front yards' beds along the houses: dark bark mulch.
const BED_TINT: [f32; 3] = [0.36, 0.33, 0.8];
/// The slope south of Gaviota's south row down toward Gardenia, and the bank's lower
/// third: green groundcover under the trees, a little darker than the lawns.
const SHADED_COVER: [f32; 3] = [0.4, 0.72, 1.7];
const BANK_FOOT: [f32; 3] = [0.38, 0.66, 1.6];
const SAGE: [f32; 3] = [0.35, 0.5, 1.5];
/// The bank under Senda Parkway: grey-green groundcover drifting over bark mulch, and how
/// much of it is groundcover at least and at most.
const BANK_COVER: [f32; 3] = [0.38, 0.43, 1.0];
const BANK_PLANTED: (f64, f64) = (0.8, 0.95);
/// How much of the other planted slopes (round the tract) is groundcover, at least and
/// at most.
const SLOPE_PLANTED: (f64, f64) = (0.7, 0.9);
/// The planted ground's tints over the packed-dirt tile, linear (it is an orange tan, so
/// they draw its red down and its blue up): bark mulch, the sage grey-green groundcover
/// drifting through it, the shaded groundcover under the slopes' trees and the front
/// beds' darker mulch; it reaches this far (m) past the arena's edge, in cells this fine
/// (m).
const DIRT_MULCH: [f32; 3] = [0.3, 0.34, 0.85];
const DIRT_COVER: [f32; 3] = [0.26, 0.56, 1.1];
const DIRT_SHADE: [f32; 3] = [0.2, 0.44, 0.9];
const DIRT_BED: [f32; 3] = [0.22, 0.25, 0.7];
/// The groundcover on the greenbelt's mouth, grey-green senecio.
const MOUTH_COVER: [f32; 3] = [0.3, 0.8, 1.9];
/// The bank's bark mulch, grey-brown rather than the slopes' warmer brown.
const DIRT_BANK_MULCH: [f32; 3] = [0.4, 0.68, 2.1];
/// How much of the ground behind the outer rows is groundcover rather than shade, at
/// least and how much more it drifts.
const UNDER_TREES_COVER: (f64, f64) = (0.55, 0.35);
const PLANTED_REACH: f64 = 12.0;
const PLANTED_STEP: f64 = 1.5;
/// The foothills' ground over the dry-grass tile: tan dry grass, gold where it faces the
/// low sun, olive scrub and dark chaparral.
const FOOTHILL_TAN: [f32; 3] = [0.92, 1.0, 2.5];
const FOOTHILL_GOLD: [f32; 3] = [1.08, 1.08, 2.4];
const FOOTHILL_OLIVE: [f32; 3] = [0.5, 0.6, 1.25];
const FOOTHILL_CHAPARRAL: [f32; 3] = [0.17, 0.24, 0.42];
/// The fold oaks' crowns (radius range, m) and leaves.
const FOLD_OAK_CROWN: (f64, f64) = (1.7, 2.6);
const FOLD_OAK_LEAVES: [u32; 3] = [0x3c4a2c, 0x46532f, 0x515a36];
/// The natural hillside's grass where the winter rains leave it green.
const HILLS: [f32; 3] = [0.4, 0.62, 0.78];
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
/// The terrace walk's top and its spurs' (m), the spurs a few millimetres lower as they
/// start under the terrace walk's edge, this far (m) in; and how far over each walk its
/// paler edges lie.
const BANK_WALK_TOP: f64 = 0.03;
const BANK_SPUR_TOP: f64 = 0.024;
const BANK_SPUR_UNDER: f64 = 0.1;
const BANK_WALK_EDGE_LIFT: f64 = 0.003;
const BANK_SPUR_WALL_ROOM: f64 = 0.05;
/// How far (m) to either side a spur across the meadow swings in its switchbacks.
const BANK_SWITCHBACK: f64 = 4.0;
/// The trails on the hillside east of the cul-de-sacs: each keeps this far (m) out from
/// the neighbourhood's east edge (the back fences round the cul-de-sacs and the
/// turning circles), so it curves round the ends of Rumbo and Gaviota as the real
/// ones do, smoothed over this many metres, from and to these arena z; and how far a
/// trail keeps from a spawn pad's middle.
const HILLSIDE_TRAIL_REACH: [f64; 2] = [6.0, 25.0];
const HILLSIDE_TRAIL_SMOOTH: f64 = 6.0;
const HILLSIDE_TRAIL_SPAN: (f64, f64) = (-70.0, 72.0);
const TRAIL_SPAWN_ROOM: f64 = 4.5;
/// How far (m) a hillside trail's middle keeps from the face of the wall along the
/// arena's east edge.
const TRAIL_WALL_ROOM: f64 = 1.0 + TRAIL_WIDTH / 2.0;
/// The shrubs that grow in drifts along the hillside trails: how far (m) from a trail's
/// middle.
const TRAIL_SHRUBS: (f64, f64) = (1.0, 2.4);
/// The lots' yards inside their walls: decomposed granite over the side yards, and a
/// patio paving the back yard from the house's back wall (concrete and pavers by turns)
/// out to the planter strip of bark mulch along the back wall.
const YARD_GRAVEL: u32 = 0x9d907a;
const PATIO_CONCRETE: u32 = 0xd2cbbd;
const PATIO_PAVERS: u32 = 0xb59f86;
/// The back yards' planting: low plants along the planter strip, about one every this many
/// metres with a gap now and then (this share of the spots bare), their spread (radius,
/// m); and the leaves of the potted plant on a patio, dark glossy greens or a red-leaved
/// one.
const YARD_SHRUB_STEP: f64 = 0.8;
const YARD_SHRUB_GAPS: f64 = 0.2;
const YARD_SHRUB: (f64, f64) = (0.17, 0.21);
const YARD_SHRUB_LEAVES: [u32; 4] = [0x3f5a35, 0x4a6238, 0x55603f, 0x7a4a52];
/// The planter strip's plants: this tall for their spread, low and leafy, in these greens
/// (a red-leaved one read as a red ball against the mulch).
const YARD_BED_SQUASH: f64 = 0.7;
const YARD_BED_LEAVES: [u32; 3] = [0x3f5a35, 0x4a6238, 0x55603f];
/// The bark mulch along the outside of every back wall, at the foot of the bank and the
/// slopes (its width, m).
const WALL_FOOT: f64 = 1.0;
/// The rear loggias' tile courses: their spacing down the slope and the butts' depth (m).
const LOGGIA_TILE_COURSE: (f64, f64) = (0.32, 0.03);
/// How much darker than the tile its courses' shadowed butts are.
const LOGGIA_BUTT_SHADE: f64 = 0.72;
const PERGOLA_SLAT: (f64, f64) = (0.07, 0.32);
/// A round spa on some yards at least this deep (m): its radius and height (m), its
/// coping and its water.
const SPA_YARD: f64 = 2.5;
const SPA: (f64, f64) = (0.6, 0.3);
const SPA_COPING: u32 = 0xc9bfac;
const SPA_WATER: u32 = 0x4f7d86;
/// A small patio table under an umbrella on some lots: the table's radius, height (m)
/// and top, the canopy's radius and the pole's height (m), and the canvas, cream or
/// terracotta.
const TABLE: (f64, f64, u32) = (0.3, 0.4, 0x5a5550);
const UMBRELLA: (f64, f64) = (0.62, 1.3);
const UMBRELLA_CANVAS: (u32, u32) = (0xd9cfb2, 0x9c4a36);
/// A terracotta pot on some patios: its radius and height (m) and paint, and its shrub's
/// radius (m).
const POT: (f64, f64, u32) = (0.2, 0.3, 0xa65e3e);
const POT_PLANT: f64 = 0.24;
/// The mown lawns' tint (the front lawns, the parkways and the bend park's) over the
/// packed-dirt tile, linear: a calm mid green. That tile's fine, low-contrast grain
/// reads as close-mown turf; the dry-grass tile's leafy straw, pushed green, shimmered as
/// the camera moved.
const FRONT_LAWN_TINT: [f32; 3] = [0.23, 0.64, 0.95];
/// The turning circles' islands: a lower, darker groundcover over the same tile.
const ISLAND_GROUNDCOVER_TINT: [f32; 3] = [0.19, 0.52, 0.72];
/// The mottle over a lawn: how much its brightness swells and falls over a few metres,
/// and from cell to cell.
const LAWN_MOTTLE: (f64, f64) = (0.07, 0.04);
/// The house models' entry passage along the front corner away from the garage, its
/// width on a [`HOUSE_WIDTH`] front (the models scale with their width), and how far (m)
/// the entry walk runs into it under the steps.
const ENTRY_PASSAGE: f64 = 1.45;
const WALK_INTO_PASSAGE: f64 = 0.6;
/// How far (m) the office's tile eave overhangs its front: the porch patch reaches the
/// eave, the planting bed starts at the wall under it.
const OFFICE_EAVE: f64 = 0.3;
/// The planting bed along the office's front (its depth, m), and the least lawn (m) it
/// leaves before the sidewalk.
const OFFICE_BED: f64 = 0.32;
const LEAST_FRONT_LAWN: f64 = 0.12;
/// The decomposed granite of the gravel front yards.
const FRONT_GRAVEL: u32 = 0xb4a68c;
/// The front yards' shrubs: clipped cones (radius at the foot, height range, m), rounded
/// mounds (radius range, m), in dark glossy greens, and the gravel yards' small agaves
/// (size range, m).
const FRONT_CONE_RADIUS: f64 = 0.15;
const FRONT_CONE: (f64, f64) = (0.5, 0.72);
const FRONT_MOUND: (f64, f64) = (0.16, 0.22);
const FRONT_SHRUB_LEAVES: [u32; 3] = [0x34502f, 0x3f5a35, 0x4a6238];
/// The cones' tiers spread this much wider than their foot's radius; the share of the
/// front beds' mounds in flower, and their blossoms: daylily orange, salvia red.
const FRONT_CONE_SPREAD: f64 = 1.25;
const FRONT_FLOWERING: f64 = 0.3;
const FRONT_BLOSSOM: [u32; 2] = [0xd8782c, 0xb8382c];
const FRONT_AGAVE: (f64, f64) = (0.22, 0.3);
/// The little park on the outside of Neblina's bend: its kidney lawn's half width and
/// half depth as shares of the park's, and the bite out of its street side; the walk's
/// width (m); the bark mulch round it (a tint over the packed-dirt tile, linear); its
/// benches' length and seat height (m) and wood.
const PARK_LAWN: (f64, f64, f64) = (0.5, 0.48, 0.28);
const PARK_WALK: f64 = 1.2;
const PARK_MULCH_TINT: [f32; 3] = [0.25, 0.32, 0.68];
const BENCH: (f64, f64) = (1.3, 0.45);
const BENCH_WOOD: u32 = 0x5a4632;
/// The planted bed outside a row's end lot's view fence toward the street or the
/// hillside, as the corner lots' are: tall ornamental grass (lomandra) over a grey-green
/// groundcover. The bed's width and how far (m) its pieces run, the groundcover's paint
/// (a bluer grey read as teal under the sky's cool fill), and the grass: a tuft every this
/// far (m) along each of its two staggered lines, each tuft's blades' width, its height
/// range (m), its blades and their leaves.
const SIDE_BED: (f64, f64) = (1.3, 1.0);
const SIDE_BED_COVER: u32 = 0x666c52;
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
/// The greenbelt's path: a walk of packed bark mulch down its lane's middle, from its
/// mouth on Neblina to the hillside: its width and its mulch.
const PATH_WIDTH: f64 = 1.2;
const PATH_MULCH: u32 = 0x86705a;
/// A paseo's bands out from its middle (m): the concrete walk's half width, the
/// decomposed granite's outer edge and the planting strip's, just short of the walls;
/// its walk's score lines, one every so far (m), and their colour; the granite; and the
/// strips' shrubs, one every so far (m) along each side, their radius range (m).
const PASEO_BANDS: (f64, f64, f64) = (1.1, 2.35, 3.2);
const PASEO_SCORE: f64 = 1.5;
const PASEO_JOINT: u32 = 0x8d887c;
const PASEO_GRANITE: u32 = 0xb4a68c;
const PASEO_SHRUB_STEP: f64 = 1.5;
const PASEO_SHRUB: (f64, f64) = (0.22, 0.32);
/// Nothing planted or grown stands within this far (m) of the greenbelt lane's middle:
/// it is a tank's way between the back walls.
const LANE_CLEAR: f64 = 2.25;
/// The greenbelt's groundcover and its low shrubs, drawn only (a tank drives through
/// them), grow to this far (m) from its walk's middle: just off the walk's mulch.
const MAT_LANE_CLEAR: f64 = PATH_WIDTH / 2.0 + 0.4;
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
/// Acorn street lamps ([`lamp_sites`]): a dark pole with a pale acorn globe under a cap
/// (m, paints).
const LAMP_POLE: f64 = 0.055;
const LAMP_GLOBE: f64 = 0.16;
const LAMP_METAL: u32 = STREET_LAMP_COLORS[0];
const LAMP_GLASS: u32 = 0xf2eee2;
/// Senda Parkway's tall lamps' globe (m).
const WAY_LAMP_GLOBE: f64 = 0.22;
/// Trees outside the walls: one every this many metres on the slopes either side of
/// Senda Parkway, and the hillside's few oaks among its sage.
const SLOPE_TREE_SPACING: f64 = 11.0;
const HILLSIDE_OAK_SPACING: f64 = 26.0;
/// The belt of oaks past the level's south and west walls, in loose groves with open
/// grass between rather than an orchard's rows: its rows stand this far (m) outside the
/// wall, an oak every [`BELT_SPACING`] along it in each where the hillside's patch noise
/// passes that row's [`BELT_GROVES`] level (the outermost row on one place in
/// [`BELT_OUTER_EVERY`] only), set off its place by up to [`BELT_WANDER`] (m) either way
/// and no nearer another than [`BELT_ROOM`]; their crowns' and heights' ranges (m).
const BELT_ROWS: [f64; 3] = [4.5, 10.0, 16.0];
const BELT_GROVES: [f64; 3] = [0.6, 0.35, 0.2];
const BELT_OUTER_EVERY: usize = 3;
const BELT_SPACING: f64 = 10.0;
const BELT_WANDER: f64 = 3.0;
const BELT_ROOM: f64 = 3.4;
const BELT_CROWN: (f64, f64) = (3.0, 4.2);
const BELT_HEIGHT: (f64, f64) = (4.4, 6.0);
/// Its sage: a clump every this many metres, from this near the wall (m) out to the
/// belt's depth, where the hillside's patch noise passes [`BELT_SAGE_PATCH`].
const BELT_SAGE_SPACING: f64 = 6.5;
const BELT_SAGE_REACH: (f64, f64) = (1.6, 17.0);
const BELT_SAGE_PATCH: f64 = 0.6;
/// The belt keeps this far (m) from Senda Parkway's path where the Way leaves the level.
const BELT_WAY_ROOM: f64 = 4.0;
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
/// The greenbelt's planted slope between the back walls: low groundcover (myoporum,
/// rosemary) from wall to wall but for its walk, drawn as one surface over a grid this
/// fine (m); its mounds rise this high at least and at most (m), rippled by this much (m)
/// of fine texture, in winter greens with darker shade in its hollows (tints over the
/// dry-grass tile, linear, as the ground's); it rises over this many metres from this far
/// off the walls, the walk, a pickup and a spawn pad.
const MAT_STEP: f64 = 0.5;
const MAT_HEIGHT: (f64, f64) = (0.06, 0.22);
/// The thicket's shrubs' spacing (m): each a dome about this wide.
const MAT_DOME: f64 = 1.4;
const MAT_RIPPLE: f64 = 0.05;
const MAT_TONES: [[f32; 3]; 3] = [[0.12, 0.26, 0.4], [0.15, 0.31, 0.5], [0.09, 0.2, 0.3]];
const MAT_SHADE: [f32; 3] = [0.05, 0.13, 0.15];
/// The greenbelt slopes up from Gaviota's north row to Rumbo's south row: its groundcover
/// stands this share of its height at the Gaviota side's walk and this much taller (m)
/// against Rumbo's back fences.
const MAT_SLOPE: (f64, f64) = (0.8, 0.12);
const MAT_RISE: f64 = 0.7;
const MAT_WALL_ROOM: f64 = 0.1;
/// The shrubs standing out of the groundcover in drifts (where a slow noise passes
/// [`MAT_BUSH_DRIFT`]): one every this many metres or so there, their radius range (m) and
/// leaves, dark toyon green, a yellow green, coyote brush's grey green.
const MAT_BUSH_SPACING: f64 = 1.25;
const MAT_BUSH_DRIFT: f64 = 0.25;
const MAT_BUSH: (f64, f64) = (0.3, 0.55);
const MAT_BUSH_LEAVES: [u32; 5] = [0x46523a, 0x8a9150, 0x6f7a5c, 0x3e4a34, 0x5b6a3e];
/// A few small trees down the greenbelt's middle (scenery a tank pushes through): one
/// every this many metres along it, their crowns' breadth and their heights at least and
/// at most (m), kept this far (m) from a spawn pad, a pickup and the greenbelt's cover
/// trees.
const CREEK_TREE_STEP: f64 = 17.0;
const CREEK_TREE_CROWN: (f64, f64) = (1.8, 2.4);
const CREEK_TREE_HEIGHT: (f64, f64) = (2.7, 3.3);
const CREEK_TREE_ROOM: f64 = 3.5;
/// The broadleaf family [`tree_proportions`] grows as a pale-barked, open-crowned tree.
const PALE_BROADLEAF: u32 = 4;
/// The line the greenbelt's trees follow down its middle, meandering a little: kept this
/// wide (m) off the walls and the walk, its points this far apart (m), swaying this far (m)
/// either side of the lane's middle on two waves (radians per metre).
const CREEK: f64 = 0.45;
const CREEK_STEP: f64 = 0.45;
const CREEK_MEANDER: (f64, f64, f64) = (1.4, 0.21, 0.53);
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
/// How far (m) a planted mat's surface sits below its mounds' height, so where it thins
/// out it sinks under the ground; and over how much height above the ground it takes on
/// its leaves' colour from the ground's.
const MAT_SINK: (f64, f64) = (0.08, 0.12);
/// Over how many metres the outer slopes' mat slopes down to a street or a paseo.
const OUTER_MAT_TAPER: f64 = 2.6;
/// The slopes' shrubs' spacing (m): rosemary and salvia mounds on the west slope, broader
/// scrub on the south one.
const WEST_DOME: f64 = 1.1;
const SOUTH_DOME: f64 = 1.6;
const OUTER_PAD_ROOM: f64 = 2.3 + 2.5;
const OUTER_PICKUP_ROOM: f64 = 2.0;
const WEST_MAT_TONES: [[f32; 3]; 3] = [[0.15, 0.35, 0.56], [0.18, 0.4, 0.66], [0.12, 0.28, 0.45]];
const WEST_MAT_SHADE: [f32; 3] = [0.05, 0.09, 0.11];
/// Behind Neblina's west row the mat stands full in a planted strip this deep (m) from
/// the walls' mulch, fading over its edge to this height at least and at most (m); past
/// it the field is the tract's park, mown lawn ([`west_park`]).
const WEST_BAND: f64 = 2.5;
const WEST_COVER: (f64, f64) = (0.14, 0.34);
/// The groundcover down the slope behind Gaviota's south row toward Gardenia, under its
/// trees: a fresh green, a little taller than the west slope's low cover, kept off the
/// lane along the level's south wall ([`SOUTH_LANE`], as the band of oaks is) and sinking
/// into the ground over this many metres before it, so its edge does not stand up as a
/// sunlit bank.
const SOUTH_MAT: (f64, f64) = (0.25, 0.6);
const SOUTH_MAT_TONES: [[f32; 3]; 3] = [[0.17, 0.44, 0.44], [0.2, 0.5, 0.52], [0.14, 0.37, 0.36]];
const SOUTH_MAT_SHADE: [f32; 3] = [0.06, 0.14, 0.1];
const SOUTH_MAT_EDGE: f64 = 2.0;
/// The ground under it and along the lane: its tones this much darker, as its own leaves
/// shade it.
const SOUTH_GROUND_SHADE: f32 = 0.72;
/// The bigger shrubs standing out of the west slope's mat: one every so many metres on a
/// jittered grid (some sites left bare), their size range (m) and leaves: grey-green
/// rosemary and sage.
const WEST_SHRUB_SPACING: f64 = 1.9;
const WEST_SHRUB: (f64, f64) = (0.5, 0.8);
const WEST_SHRUB_LEAVES: [u32; 4] = [0x4e5a3c, 0x5a6447, 0x46523a, 0x626b4f];
/// Olive trees along the foot of Neblina's west row's rear walls, behind one lot in this
/// many, this far (m) out from the wall: grey-green crowns (breadth and height range, m)
/// on twisted dark trunks.
const WEST_OLIVE_EVERY: usize = 3;
const OLIVE_OUT: f64 = 2.2;
const OLIVE_CROWN: (f64, f64) = (2.4, 3.2);
const OLIVE_HEIGHT: (f64, f64) = (3.0, 3.8);
const OLIVE_LEAVES: [u32; 3] = [0x5c6450, 0x535b48, 0x646b56];
const OLIVE_BARK: u32 = 0x4a4036;
/// The hillside's scrub gathers in patches where a slow noise passes this level.
const HILL_PATCH: f64 = 0.35;
/// The hillside's coastal sage inside the arena: a clump every this many metres, its
/// middle this far (m) from the view fence's line, so its lumps leave the fence's foot
/// clear by about a metre and a half.
const HILL_SAGE_SPACING: f64 = 2.0;
const FENCE_SAGE_ROOM: f64 = 3.4;
/// Past the arena's edge, where it only closes the view, open grass with a shrub every
/// several metres in the patches: a clump every this many metres where the patch noise
/// passes this level, leafy within this far (m) of the arena's edge and painted past it.
const OUTER_SAGE_SPACING: f64 = 6.0;
const OUTER_SAGE_PATCH: f64 = 0.7;
const OUTER_SAGE_LEAFY: f64 = 15.0;
/// How dark the far sage's painted lumps are beside its leaves.
const PAINTED_SHADE: f64 = 0.82;
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
/// The turning circles' islands: low shrubs in a ring this share of the island's radius
/// out from its tree, about this far apart (m), this size range (radius, m), over the
/// island's groundcover ([`ISLAND_GROUNDCOVER_TINT`]).
const ISLAND_RING: f64 = 0.62;
const ISLAND_SHRUB_STEP: f64 = 0.9;
const ISLAND_SHRUB: (f64, f64) = (0.22, 0.32);
/// The dark bark mulch of the planting beds.
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
    // Built in the level's design metres and standing SENDA_SCALE times larger about the
    // arena's middle, as its cover does; the spawn pads are the vehicles' size, at the
    // simulation's spawns.
    let mut design = Node::group("");
    let hills = Hills::new();
    design.children.push(grass(&hills));
    design.children.push(planted_ground(&Lines::new()));
    design.children.push(paving());
    design.children.push(paseo_grounds());
    design.children.push(slope_trees(&hills));
    design.children.push(greenbelt_scrub());
    design.children.push(outer_slopes());
    design.children.push(shrubs());
    design.children.push(bank_planting());
    design.children.push(yard_props());
    design.children.push(street_lamps());
    design.scale = DVec3::splat(SENDA_SCALE);
    let mut pads = Node::group("");
    pads.children.push(create_spawn_pads(SENDA_SCALE));
    batch(&mut pads);
    let mut root = Node::group("senda-scenery");
    root.children.push(design);
    root.children.push(pads);
    root
}

/// The ground: dry-grass ground watered green over the lawns, parkways and the
/// greenbelt, planted beds in the front yards, groundcover along Senda Parkway, left as
/// sage scrub on the slopes, rising past the arena east and south into the green
/// rolling hills every street of the neighbourhood looks out on. Its tile repeats
/// ([`repeated_ground_material`]) and turns off the streets' grid, so a large stretch of
/// it shows no mirror lines; the park's mown lawn is the planted ground's.
fn grass(hills: &Hills) -> Node {
    let segments = (GROUND_EXTENT / GROUND_STEP).round() as u32;
    let mut geometry = plane_geometry_segments(GROUND_EXTENT, GROUND_EXTENT, segments, segments);
    geometry.rotate_x(-FRAC_PI_2);
    for p in &mut geometry.positions {
        p[1] = hills.height(f64::from(p[0]), f64::from(p[2])) as f32;
    }
    geometry.compute_vertex_normals();
    ground_uvs(&mut geometry, 0.0, 0.0);
    let (sin, cos) = GRASS_TILE_TURN.sin_cos();
    for uv in &mut geometry.uvs {
        let [u, v] = uv.map(f64::from);
        *uv = [(u * cos - v * sin) as f32, (u * sin + v * cos) as f32];
    }
    let lines = Lines::new();
    geometry.colors = geometry
        .positions
        .iter()
        .zip(&geometry.normals)
        .map(|(p, normal)| {
            let at = DVec2::new(f64::from(p[0]), f64::from(p[2]));
            ground_tint(at, *normal, hills, &lines)
        })
        .collect();
    let mut surface = repeated_ground_material(GroundKind::DryGrass);
    surface.vertex_colors = true;
    surface.color = crate::scene::Color(0xffffff);
    let mut floor = shadow_receiver(Arc::new(geometry), Arc::new(surface));
    floor.position.y = GRASS_LIFT;
    floor
}

/// How far (rad) the ground's tile turns off the world's axes, so its repeats do not run
/// along the streets and walls.
const GRASS_TILE_TURN: f64 = 0.41;
/// How far (m) the ground and the planted ground over it stand off the arena's floor.
const GRASS_LIFT: f64 = 0.008;
const PLANTED_LIFT: f64 = 0.014;
/// The dry-grass tile's mean colour over the packed-dirt tile's (linear, each channel):
/// a tint over the dry grass times this is the same colour, on the whole, over the dirt.
const DRY_GRASS_OVER_DIRT: [f32; 3] = [0.527, 0.747, 0.601];

/// The ground's vertex colour at `at`, where its surface faces `normal`: [`grass_tint`],
/// turning to the hills' [`foothill_tint`] as they rise, mottled gently over a few metres
/// as a meadow is, its thicker and thinner patches.
fn ground_tint(at: DVec2, normal: [f32; 3], hills: &Hills, lines: &Lines) -> [f32; 3] {
    let tint = grass_tint(at, lines);
    let share = hills.share(at.x, at.y) as f32;
    let tint: [f32; 3] = if share <= 0.0 {
        tint
    } else {
        let foothill = foothill_tint(at, normal);
        std::array::from_fn(|i| tint[i] + (foothill[i] - tint[i]) * share)
    };
    // Smooth in `at`, not hashed, so the planted ground's edge, which works out the
    // ground's colour between its vertices ([`grass_drawn`]), finds the same values.
    let (swell, grain) = GROUND_MOTTLE;
    let mottle = 1.0
        + swell * (at.x * 0.37 + at.y * 0.23).sin() * (at.y * 0.31 - at.x * 0.17 + 0.8).cos()
        + grain * (at.x * 2.31 + at.y * 1.73).sin() * (at.y * 2.93 - at.x * 1.07 + 0.4).cos();
    tint.map(|c| c * mottle as f32)
}

/// The ground's mottle: how much its brightness swells and falls over ten metres or so,
/// and from vertex to vertex.
const GROUND_MOTTLE: (f64, f64) = (0.06, 0.05);

/// The colour the ground ([`grass`]) is drawn in at `p` on the level's floor, as a tint
/// over the packed-dirt tile: its vertices' [`ground_tint`] as its triangles blend them.
fn grass_drawn(p: DVec2, hills: &Hills, lines: &Lines) -> [f32; 3] {
    let segments = (GROUND_EXTENT / GROUND_STEP).round();
    let step = GROUND_EXTENT / segments;
    let half = GROUND_EXTENT / 2.0;
    let (gx, gz) = ((p.x + half) / step, (p.y + half) / step);
    let (ix, iz) = (gx.floor(), gz.floor());
    let (u, v) = (gx - ix, gz - iz);
    // The vertex as the plane stores it (f32), so the colour is the vertex's own.
    let corner = |i: f64, j: f64| {
        let at = DVec2::new(
            f64::from((i * step - half) as f32),
            f64::from((j * step - half) as f32),
        );
        ground_tint(at, [0.0, 1.0, 0.0], hills, lines)
    };
    // Each cell splits along the diagonal from its (x0, z1) corner to its (x1, z0) one.
    let (a, b, d) = (corner(ix, iz), corner(ix, iz + 1.0), corner(ix + 1.0, iz));
    let drawn: [f32; 3] = if u + v <= 1.0 {
        std::array::from_fn(|i| a[i] + (d[i] - a[i]) * u as f32 + (b[i] - a[i]) * v as f32)
    } else {
        let c = corner(ix + 1.0, iz + 1.0);
        std::array::from_fn(|i| {
            c[i] + (b[i] - c[i]) * (1.0 - u) as f32 + (d[i] - c[i]) * (1.0 - v) as f32
        })
    };
    std::array::from_fn(|i| drawn[i] * DRY_GRASS_OVER_DIRT[i])
}

/// How much of the planted ground's colour lies at `p`, from 0 to 1: all of it inside
/// the planted zones, none past them, fading over the zones' own metre of edge along a
/// line that wanders a little.
fn planted_weight(p: DVec2, lines: &Lines) -> f64 {
    let zone = zones(p, lines);
    // Outside the level's wall only the bank under Senda Parkway is planted: the natural
    // grass runs up to the wall everywhere else.
    let open = (1.0 - zone.lawn).max(zone.front)
        * (1.0 - (zone.greenbelt - zone.mouth).max(0.0))
        * (1.0 - zone.hillside)
        * (1.0 - zone.beyond_wall)
        * (1.0 - zone.meadow);
    let (middle, wander) = PLANTED_EDGE;
    let uneven = 0.6 * (p.x * 1.37 + p.y * 0.91).sin() * (p.y * 1.21 - p.x * 0.53 + 0.3).cos()
        + 0.4 * (p.x * 2.9 - p.y * 2.3 + 1.1).sin();
    rise((open - middle) / middle + 0.5 + wander * uneven)
}

/// What the ground looks like at `p` from above, as a tint over the packed-dirt tile:
/// the planted ground where it lies, fading into the ground's own colour at its edges.
fn ground_seen(p: DVec2, hills: &Hills, lines: &Lines) -> [f32; 3] {
    let w = planted_weight(p, lines) as f32;
    let planted = planted_tint(p, lines);
    if w >= 1.0 {
        return planted;
    }
    let ground = grass_drawn(p, hills, lines);
    std::array::from_fn(|c| ground[c] + (planted[c] - ground[c]) * w)
}

/// The planted ground laid over the grass: bark mulch with groundcover drifting through
/// it on the bank below Senda Parkway and across it, in the field behind Neblina's west
/// row, the slope behind Gaviota's south row and the greenbelt's mouth, dark mulch in the
/// front yards' beds and the park's mown lawn; everywhere but the lots' yards, the
/// greenbelt's scrub, the hillside's natural grass, the hills and the paving. Square cells
/// of a grid where it lies, in the ground material the lawns share ([`lawn_material`]),
/// so the planted slopes read as mulch and groundcover and the park as close-mown turf.
/// Its edge on open ground fades over a cell into the colour the ground below is drawn in
/// ([`grass_drawn`]), along a line that wanders a little, so it shows as a soft, uneven
/// drift rather than as the cells' staircase; at the paving it keeps its colour to the
/// cell that reaches under the slabs, so its edge never shows there, and it never lies
/// over them.
fn planted_ground(lines: &Lines) -> Node {
    let hills = Hills::new();
    let reach = ARENA + PLANTED_REACH;
    let cells = (2.0 * reach / PLANTED_STEP).round() as usize;
    let corner = |i: usize, j: usize| {
        DVec2::new(
            -reach + i as f64 * PLANTED_STEP,
            -reach + j as f64 * PLANTED_STEP,
        )
    };
    let weights: Vec<f64> = (0..(cells + 1) * (cells + 1))
        .map(|k| planted_weight(corner(k % (cells + 1), k / (cells + 1)), lines))
        .collect();
    // A cell lies where it is planted off the paving; one that reaches under a slab keeps
    // its colour there, so its edge falls under the slab.
    let planted: Vec<bool> = (0..(cells + 1) * (cells + 1))
        .map(|k| {
            let p = corner(k % (cells + 1), k / (cells + 1));
            weights[k] > 0.0 && !is_senda_paved(p.x, p.y)
        })
        .collect();
    let mut index = vec![u32::MAX; (cells + 1) * (cells + 1)];
    let mut positions: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for j in 0..cells {
        for i in 0..cells {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            if !corners.iter().any(|&(i, j)| planted[j * (cells + 1) + i]) {
                continue;
            }
            let quad = corners.map(|(i, j)| {
                let k = j * (cells + 1) + i;
                if index[k] == u32::MAX {
                    let p = corner(i, j);
                    positions.extend([p.x, 0.0, p.y]);
                    colors.push(ground_seen(p, &hills, lines));
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
    let mut node = shadow_receiver(Arc::new(mesh), lawn_material());
    node.position.y = PLANTED_LIFT;
    node
}

/// Where the planted ground's edge lies: at this share of open ground (between the
/// lots, the greenbelt, the hillside and the meadow), its colour fading in over the
/// zones' own metre of edge, the line wandering by up to this much of that metre either
/// way.
const PLANTED_EDGE: (f64, f64) = (0.5, 0.45);

/// The planted ground's tint at `p` over the packed-dirt tile, linear: bark mulch with
/// groundcover drifting through it (more of it down the bank and round the tract), the
/// shaded groundcover of the slope behind Gaviota's south row and of the bank's foot,
/// the front beds' dark mulch and the park's mown lawn.
fn planted_tint(p: DVec2, lines: &Lines) -> [f32; 3] {
    let zone = zones(p, lines);
    let park = lawn_tone(p, WEST_PARK_LAWN);
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
        // The slope behind Gaviota's south row: the green of its groundcover, which sinks
        // into it short of the lane along the wall, so its edge does not show.
        let south = mix(
            SOUTH_MAT_TONES[2][i],
            SOUTH_MAT_TONES[1][i],
            0.3 + 0.5 * noise,
        ) * SOUTH_GROUND_SHADE;
        let planted = mix(planted, south, zone.south);
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
        let planted = mix(planted, DIRT_BED[i], zone.front);
        // The park's mown lawn, mottled as the front lawns are.
        mix(planted, park[i], zone.park)
    })
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
    /// The bank's east end, where it widens into dry meadow.
    meadow: f64,
    /// The planted field behind Neblina's west row, and the slope behind Gaviota's
    /// south row.
    west: f64,
    south: f64,
    /// The park's mown lawn over the field behind Neblina's west row, past its planted
    /// strip.
    park: f64,
    /// The natural hillside east of the cul-de-sacs.
    hillside: f64,
    /// Outside the level's wall, but for the bank under Senda Parkway north of it:
    /// natural grass.
    beyond_wall: f64,
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
    let beyond_wall = if inside_boundary(&lines.boundary, p) {
        0.0
    } else if p.y < -ARENA {
        1.0 - bank_land
    } else {
        1.0
    };
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
        meadow: bank_land * ((p.x - BANK_MEADOW.0) / BANK_MEADOW.1).clamp(0.0, 1.0),
        west: outer - south,
        south,
        park: (outer - south) * (1.0 - west_band(p, lines)) * (1.0 - beyond_wall),
        hillside: ((p.x - HILLSIDE_FROM) / HILLSIDE_BLEND).clamp(0.0, 1.0),
        // The bank runs on outside the north wall under the Way; west of the level the
        // Way has ended at the wall.
        beyond_wall,
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
    let parkway = under_parkway(p, lines);
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
        let wild = zone.hillside.max(zone.beyond_wall).max(zone.meadow);
        let slope = mix(planted, natural, wild) * (1.0 - BANK_SHADE * zone.bank as f32);
        let greenbelt = mix(GREENBELT_TONES[2][i], GREENBELT_TONES[0][i], mottle * 0.6);
        let greenbelt = mix(greenbelt, GROUNDCOVER[i], zone.mouth);
        let tint = mix(slope, greenbelt, zone.greenbelt);
        let tint = mix(tint, GROUNDCOVER[i], zone.groundcover);
        let tint = mix(tint, LAWN[i], zone.lawn);
        let tint = mix(
            tint,
            WEST_PARK_LAWN[i] / DRY_GRASS_OVER_DIRT[i] * (0.92 + 0.16 * noise as f32),
            zone.park,
        );
        let beds = mix(
            BED_TINT[i],
            GROUNDCOVER[i],
            FRONT_BEDS_COVER * (0.6 + 0.8 * patch),
        );
        let tint = mix(tint, beds, zone.front);
        // Under the residential streets' parkways and sidewalks, the clipped lawn the
        // lots' parkway strips are, so the wedges the slabs leave read as parkway.
        let tint = mix(tint, FRONT_LAWN_TINT[i] / DRY_GRASS_OVER_DIRT[i], parkway);
        tint * (0.94 + 0.06 * patch as f32)
    })
}

/// How far `p` lies under a residential street's parkway and sidewalk or its turning
/// circle, from 0 just past the back of the sidewalk to 1 a little inside it.
fn under_parkway(p: DVec2, lines: &Lines) -> f64 {
    let walk = |distance: f64| rise((STREET_HALF_WALK + 0.2 - distance) / 0.4);
    let circles = [Street::Rumbo, Street::NeblinaGaviota]
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .map(|middle| rise((CUL_DE_SAC_RADIUS + 0.2 - middle.distance(p)) / 0.4))
        .fold(0.0, f64::max);
    walk(lines.rumbo.distance(p))
        .max(walk(lines.gaviota.distance(p)))
        .max(circles)
}

/// A flat ribbon along `line` between `from` and `to` metres to its left, its top at
/// `y`, leaving out the stretches `skip` says lie on something else (another street's
/// asphalt, a turning circle).
fn ribbon(line: &[DVec2], from: f64, to: f64, y: f64, skip: &dyn Fn(DVec2) -> bool) -> Mesh {
    ribbon_cut(line, from, to, y, skip, &|_| 1.0)
}

/// A [`ribbon`] cut where `keep` turns negative, exactly along the line where it is
/// nought, so it ends flush against what it meets rather than at a piece's end.
fn ribbon_cut(
    line: &[DVec2],
    from: f64,
    to: f64,
    y: f64,
    skip: &dyn Fn(DVec2) -> bool,
    keep: &dyn Fn(DVec2) -> f64,
) -> Mesh {
    let mut surface = Surface::default();
    ribbon_quads(line, from, to, |quad| {
        if !skip((quad[0] + quad[2]) / 2.0) {
            surface.polygon(&clip(&quad, keep), y);
        }
    });
    surface.mesh()
}

/// The left normal at each of `line`'s joints, between its spans either side.
fn joint_normal(line: &[DVec2], i: usize) -> DVec2 {
    let n = line.len();
    left((line[(i + 1).min(n - 1)] - line[i.saturating_sub(1)]).normalize())
}

/// The pieces a ribbon along `line` from `from` to `to` metres to its left is laid in,
/// each a quad about a metre long (long spans split so the skipped stretches end near
/// what they meet), its normal turning smoothly from joint to joint.
fn ribbon_quads(line: &[DVec2], from: f64, to: f64, mut each: impl FnMut([DVec2; 4])) {
    for i in 0..line.len() - 1 {
        let (a, b) = (line[i], line[i + 1]);
        let (na, nb) = (joint_normal(line, i), joint_normal(line, i + 1));
        let pieces = (a.distance(b) / 1.0).ceil().max(1.0) as usize;
        for k in 0..pieces {
            let (t0, t1) = (k as f64 / pieces as f64, (k + 1) as f64 / pieces as f64);
            let at = |t: f64, offset: f64| a.lerp(b, t) + na.lerp(nb, t).normalize() * offset;
            each([at(t0, from), at(t1, from), at(t1, to), at(t0, to)]);
        }
    }
}

/// The edge `offset` metres to the left of `line` that [`ribbon`]'s pieces follow, joint
/// for joint, so a cut found along it lands where the ribbon's own cut does.
fn ribbon_edge(line: &[DVec2], offset: f64) -> Line {
    let mut points = Vec::new();
    for i in 0..line.len() - 1 {
        let (a, b) = (line[i], line[i + 1]);
        let (na, nb) = (joint_normal(line, i), joint_normal(line, i + 1));
        let pieces = (a.distance(b) / 1.0).ceil().max(1.0) as usize;
        let first = if i == 0 { 0 } else { 1 };
        for k in first..=pieces {
            let t = k as f64 / pieces as f64;
            points.push(a.lerp(b, t) + na.lerp(nb, t).normalize() * offset);
        }
    }
    Line(points)
}

/// The point `s` metres along `line` and its left normal there, turning from joint to
/// joint as [`ribbon`]'s does: the point `offset` metres along the normal lies on the
/// edge of a ribbon that far to the line's left.
fn line_frame(line: &[DVec2], s: f64) -> (DVec2, DVec2) {
    let n = line.len();
    let mut rest = s.max(0.0);
    for i in 0..n - 1 {
        let span = line[i].distance(line[i + 1]);
        if rest <= span || i + 2 == n {
            let t = (rest / span).min(1.0);
            let normal = joint_normal(line, i).lerp(joint_normal(line, i + 1), t);
            return (line[i].lerp(line[i + 1], t), normal.normalize());
        }
        rest -= span;
    }
    (line[0], joint_normal(line, 0))
}

/// The part of the convex `polygon` where `keep` is not negative, its cut edges where
/// `keep`, taken as linear between corners, is nought.
fn clip(polygon: &[DVec2], keep: &dyn Fn(DVec2) -> f64) -> Vec<DVec2> {
    let values: Vec<f64> = polygon.iter().map(|&p| keep(p)).collect();
    if values.iter().all(|&value| value >= 0.0) {
        return polygon.to_vec();
    }
    let n = polygon.len();
    let mut kept = Vec::with_capacity(n + 2);
    for i in 0..n {
        let j = (i + 1) % n;
        let (p, q, fp, fq) = (polygon[i], polygon[j], values[i], values[j]);
        if fp >= 0.0 {
            kept.push(p);
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            kept.push(p + (q - p) * (fp / (fp - fq)));
        }
    }
    kept
}

/// Where `field` first changes sign along `points`, taken as linear between them as
/// [`clip`] takes it, among the stretches within `reach` (m) of `near`.
fn crossing(
    points: &[DVec2],
    field: &dyn Fn(DVec2) -> f64,
    near: DVec2,
    reach: f64,
) -> Option<DVec2> {
    points
        .windows(2)
        .filter(|pair| pair.iter().all(|p| p.distance(near) < reach))
        .find_map(|pair| {
            let (p, q) = (pair[0], pair[1]);
            let (fp, fq) = (field(p), field(q));
            ((fp >= 0.0) != (fq >= 0.0)).then(|| p + (q - p) * (fp / (fp - fq)))
        })
}

/// Triangles gathered into one flat-shaded mesh, each wound to face the way it is told,
/// with world-aligned uvs like the paving's: flat ones by their plan, upright ones like a
/// [`Wall`]'s.
#[derive(Default)]
struct Surface {
    positions: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
}

impl Surface {
    fn triangle(&mut self, [a, b, c]: [DVec3; 3], facing: DVec3) {
        let normal = (b - a).cross(c - a);
        if normal.length_squared() < 1e-12 {
            return;
        }
        let (corners, normal) = if normal.dot(facing) < 0.0 {
            ([a, c, b], -normal.normalize())
        } else {
            ([a, b, c], normal.normalize())
        };
        for p in corners {
            self.positions.extend([p.x, p.y, p.z]);
            self.normals.extend([normal.x, normal.y, normal.z]);
            if normal.y.abs() > 0.5 {
                self.uvs.extend([p.x / 4.0, p.z / 4.0]);
            } else {
                self.uvs.extend([(p.x + p.z) / 4.0, p.y / 4.0]);
            }
        }
    }

    fn quad(&mut self, [a, b, c, d]: [DVec3; 4], facing: DVec3) {
        self.triangle([a, b, c], facing);
        self.triangle([a, c, d], facing);
    }

    /// A flat convex polygon on the plan at height `y`, face up.
    fn polygon(&mut self, points: &[DVec2], y: f64) {
        let at = |p: DVec2| DVec3::new(p.x, y, p.y);
        for k in 1..points.len().saturating_sub(1) {
            self.triangle([at(points[0]), at(points[k]), at(points[k + 1])], DVec3::Y);
        }
    }

    fn mesh(self) -> Mesh {
        Mesh::from_f64(&self.positions, &self.normals, &self.uvs, None)
    }
}

/// A kerb lowered across a driveway: its top's height (m) there and how long (m) it
/// takes to slope back up to [`KERB_HEIGHT`] either side.
const DROPPED_KERB: f64 = 0.07;
const DROPPED_KERB_SLOPE: f64 = 0.35;

/// A point along a kerb: on the foot of its face, the way back across its top (toward
/// the walk), and its top's height there.
#[derive(Clone, Copy)]
struct KerbPoint {
    at: DVec2,
    back: DVec2,
    top: f64,
}

/// A kerb along `points`: its top, its face down to the asphalt, and the gutter pan
/// along its foot.
fn kerb_run(points: &[KerbPoint], kerb: &mut Surface, pan: &mut Surface) {
    let at = |p: DVec2, y: f64| DVec3::new(p.x, y, p.y);
    let gutter = GUTTER.0;
    let pan_top = STREET_TOP + GUTTER_LIFT;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (a_back, b_back) = (a.at + a.back * KERB_WIDTH, b.at + b.back * KERB_WIDTH);
        kerb.quad(
            [
                at(a.at, a.top),
                at(b.at, b.top),
                at(b_back, b.top),
                at(a_back, a.top),
            ],
            DVec3::Y,
        );
        let out = -(a.back + b.back).normalize();
        kerb.quad(
            [
                at(a.at, STREET_TOP),
                at(b.at, STREET_TOP),
                at(b.at, b.top),
                at(a.at, a.top),
            ],
            DVec3::new(out.x, 0.0, out.y),
        );
        pan.quad(
            [
                at(a.at - a.back * gutter, pan_top),
                at(b.at - b.back * gutter, pan_top),
                at(b.at, pan_top),
                at(a.at, pan_top),
            ],
            DVec3::Y,
        );
    }
}

/// The places along a kerb from `from` to `to` (metres along its street, or radians round
/// its turning circle) its pieces end, no further apart than `step` and at each of
/// `bends` (where a straight kerb's street turns), with its top's height at each:
/// lowered across each of the `drops` (a driveway's edges), sloping back up over `slope`
/// either side; where the kerb runs `straight`, with no other place across a drop.
fn kerb_profile(
    (from, to): (f64, f64),
    (step, bends, straight): (f64, &[f64], bool),
    drops: &[(f64, f64)],
    slope: f64,
) -> Vec<(f64, f64)> {
    let pieces = ((to - from) / step).ceil().max(1.0) as usize;
    let inside = |s: f64| s > from + 1e-3 && s < to - 1e-3;
    let mut places: Vec<f64> = (0..=pieces)
        .map(|k| from + (to - from) * k as f64 / pieces as f64)
        .chain(bends.iter().copied().filter(|&s| inside(s)))
        .filter(|&s| {
            !straight
                || drops
                    .iter()
                    .all(|&(a, b)| s <= a - slope || s >= b + slope || !inside(s))
        })
        .collect();
    for &(a, b) in drops {
        places.extend(
            [a - slope, a, b, b + slope]
                .into_iter()
                .filter(|&s| inside(s)),
        );
    }
    places.sort_by(f64::total_cmp);
    places.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    places
        .into_iter()
        .map(|s| {
            let top = drops
                .iter()
                .map(|&(a, b)| {
                    let past = (a - s).max(s - b).max(0.0);
                    DROPPED_KERB + (KERB_HEIGHT - DROPPED_KERB) * (past / slope).min(1.0)
                })
                .fold(KERB_HEIGHT, f64::min);
            (s, top)
        })
        .collect()
}

/// The course a stretch of kerb's foot follows: along a street on its left (1) or right
/// (-1), metres along it; round a kerb return or a flare about its middle from one
/// straight kerb's end to the next, a share of the way; round a turning circle, radians.
enum KerbCourse {
    Street {
        line: Line,
        half: f64,
        side: f64,
    },
    Corner {
        centre: DVec2,
        from: (DVec2, DVec2),
        to: (DVec2, DVec2),
    },
    Ring {
        middle: DVec2,
    },
}

/// One smooth stretch of kerb, from `from` to `to` along its course, laid in pieces no
/// longer than `step` and, along a street, at each of its joints (`bends`); how far
/// along it a metre is; and its back edge traced finely, for the driveways to find.
struct KerbRun {
    course: KerbCourse,
    from: f64,
    to: f64,
    step: f64,
    bends: Vec<f64>,
    metre: f64,
    back_edge: Vec<(f64, DVec2)>,
}

impl KerbRun {
    fn new(course: KerbCourse, (from, to): (f64, f64), step: f64) -> Self {
        let bends = match &course {
            KerbCourse::Street { line, .. } => line
                .0
                .windows(2)
                .scan(0.0, |along, pair| {
                    *along += pair[0].distance(pair[1]);
                    Some(*along)
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut run = KerbRun {
            course,
            from,
            to,
            step,
            bends,
            metre: 1.0,
            back_edge: Vec::new(),
        };
        let pieces = ((to - from) / (step / 8.0)).ceil().max(1.0) as usize;
        run.back_edge = (0..=pieces)
            .map(|k| {
                let s = from + (to - from) * k as f64 / pieces as f64;
                let (at, back) = run.frame(s);
                (s, at + back * KERB_WIDTH)
            })
            .collect();
        let length: f64 = run
            .back_edge
            .windows(2)
            .map(|pair| pair[0].1.distance(pair[1].1))
            .sum();
        run.metre = (to - from) / length.max(1e-6);
        run
    }

    /// The kerb's foot at `s` along its course and the way back across its top.
    fn frame(&self, s: f64) -> (DVec2, DVec2) {
        match &self.course {
            KerbCourse::Street { line, half, side } => {
                let (point, normal) = line_frame(&line.0, s);
                (point + normal * *side * *half, normal * *side)
            }
            KerbCourse::Corner { centre, from, to } => {
                let at = round_corner(*centre, from.0, to.0, s);
                let back = round_corner(DVec2::ZERO, from.1, to.1, s).normalize();
                (at, back)
            }
            KerbCourse::Ring { middle } => {
                let out = DVec2::from_angle(s);
                (*middle + out * CUL_DE_SAC_RADIUS, out)
            }
        }
    }

    /// The kerb's points, its top lowered across `drops` (places along its course).
    fn points(&self, drops: &[(f64, f64)]) -> Vec<KerbPoint> {
        let straight = matches!(self.course, KerbCourse::Street { .. });
        kerb_profile(
            (self.from, self.to),
            (self.step, &self.bends, straight),
            drops,
            DROPPED_KERB_SLOPE * self.metre,
        )
        .into_iter()
        .map(|(s, top)| {
            let (at, back) = self.frame(s);
            KerbPoint { at, back, top }
        })
        .collect()
    }

    /// Where a ray from `from` along `toward` first crosses the kerb's back, within
    /// `reach` (m): how far along the ray, the place along the kerb's course and the point.
    fn hit(&self, from: DVec2, toward: DVec2, reach: f64) -> Option<(f64, f64, DVec2)> {
        let cross = |u: DVec2, v: DVec2| u.x * v.y - u.y * v.x;
        self.back_edge
            .windows(2)
            .filter_map(|pair| {
                let ((sa, a), (sb, b)) = (pair[0], pair[1]);
                let span = b - a;
                let turn = cross(toward, span);
                if turn.abs() < 1e-9 {
                    return None;
                }
                let along = cross(a - from, span) / turn;
                let share = cross(a - from, toward) / turn;
                ((0.0..=reach).contains(&along) && (0.0..=1.0).contains(&share))
                    .then(|| (along, sa + (sb - sa) * share, a + span * share))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }
}

/// The longest pieces a kerb lies in along a street (m), besides one at each of its
/// joints, and round a corner (a share of the way); round a turning circle as its walk.
const KERB_STEP: f64 = 2.5;
const CORNER_STEP: f64 = 0.125;

/// Every kerb in the level as smooth runs: each street's on each side between its
/// corners, side streets' mouths and flares, the kerb returns, the flares and the turning
/// circles round from one flare to the other.
fn kerb_runs(corners: &[Corner], flares: &[Flare]) -> Vec<KerbRun> {
    let mut runs = Vec::new();
    for street in Street::ALL {
        let line = street.line();
        for side in [-1.0, 1.0] {
            for span in kerb_spans(street, side, line.length(), corners, flares) {
                let course = KerbCourse::Street {
                    line: line.clone(),
                    half: street.half(),
                    side,
                };
                runs.push(KerbRun::new(course, span, KERB_STEP));
            }
        }
    }
    for corner in corners {
        let course = KerbCourse::Corner {
            centre: corner.centre,
            from: street_kerb_foot(corner.main, corner.main_side, corner.main_at),
            to: street_kerb_foot(corner.side_street, corner.side, corner.side_at),
        };
        runs.push(KerbRun::new(course, (0.0, 1.0), CORNER_STEP));
    }
    for flare in flares {
        let Some(middle) = flare.street.cul_de_sac() else {
            continue;
        };
        let out = DVec2::from_angle(flare.angle);
        let course = KerbCourse::Corner {
            centre: flare.centre,
            from: street_kerb_foot(flare.street, flare.side, flare.at),
            to: (middle + out * CUL_DE_SAC_RADIUS, out),
        };
        runs.push(KerbRun::new(course, (0.0, 1.0), CORNER_STEP));
    }
    for street in Street::ALL {
        if let (Some(middle), Some(span)) = (street.cul_de_sac(), ring_span(street, flares)) {
            let step = std::f64::consts::TAU / RING_SEGMENTS as f64;
            runs.push(KerbRun::new(KerbCourse::Ring { middle }, span, step));
        }
    }
    runs
}

/// The foot of `street`'s kerb on `side`, `s` metres along it, and the way back across
/// its top.
fn street_kerb_foot(street: Street, side: f64, s: f64) -> (DVec2, DVec2) {
    let (point, normal) = line_frame(&street.line().0, s);
    (point + normal * side * street.half(), normal * side)
}

/// The turning circle's kerb and walk at `street`'s end, round its far side from one of
/// the street's flares to the other (radians, from and to).
fn ring_span(street: Street, flares: &[Flare]) -> Option<(f64, f64)> {
    let line = street.line();
    let (_, inward) = line.at(line.length());
    let ends: Vec<f64> = flares
        .iter()
        .filter(|flare| flare.street == street)
        .map(|flare| flare.angle)
        .collect();
    let [first, second] = ends[..] else {
        return None;
    };
    let tau = std::f64::consts::TAU;
    let sweep = (second - first).rem_euclid(tau);
    Some(
        if DVec2::from_angle(first + sweep / 2.0).dot(inward) > 0.0 {
            (first, first + sweep)
        } else {
            (second, second + tau - sweep)
        },
    )
}

/// How far (m) on from the middle of its sidewalk a driveway looks for its kerb, and how
/// far under the kerb's top its apron runs, so no sliver of ground shows between them.
const DRIVEWAY_KERB_REACH: f64 = 3.0;
const APRON_UNDER_KERB: f64 = 0.05;

/// Where lot `k`'s driveway meets its kerb: for each of [`APRON_PIECES`]` + 1` rays from
/// the middle of the sidewalk on across the parkway, its `-u` edge's first, the run it
/// meets, the place along it and the point on the kerb's back. None where any misses.
fn driveway_kerb(
    lot: &Lot,
    yard: &FrontYard,
    runs: &[KerbRun],
) -> Option<Vec<(usize, f64, DVec2)>> {
    let walk = yard.sidewalk + SIDEWALK / 2.0;
    (0..=APRON_PIECES)
        .map(|k| {
            let u = yard.garage + yard.driveway * (k as f64 / APRON_PIECES as f64 - 0.5);
            let from = lot.at(u, walk);
            runs.iter()
                .enumerate()
                .filter_map(|(index, run)| {
                    let (along, s, point) = run.hit(from, lot.front, DRIVEWAY_KERB_REACH)?;
                    Some((along, (index, s, point)))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, hit)| hit)
        })
        .collect()
}

/// The stretches of each of `runs` lowered for a driveway, from its edges' places: on
/// one run, between them; across the joint of two that meet, from each to the joint.
fn kerb_drops(runs: &[KerbRun], edges: [(usize, f64, DVec2); 2]) -> Vec<(usize, (f64, f64))> {
    let [(i, a, _), (j, b, _)] = edges;
    if i == j {
        return vec![(i, (a.min(b), a.max(b)))];
    }
    // Each run lowered from its edge on past whichever of its ends meets the other run.
    let toward = |own: &KerbRun, other: &KerbRun, s: f64| {
        let end = own.frame(own.to).0;
        let meets = [other.from, other.to]
            .iter()
            .any(|&t| other.frame(t).0.distance(end) < 0.01);
        let past = (own.to - own.from).abs();
        if meets {
            (s, own.to + past)
        } else {
            (own.from - past, s)
        }
    };
    vec![
        (i, toward(&runs[i], &runs[j], a)),
        (j, toward(&runs[j], &runs[i], b)),
    ]
}

/// Within this far (m) of a junction or a flare's corner, the walks give way to it.
const CORNER_REACH: f64 = 12.0;
/// The kerb returns' radius (m) at the street corners, the largest of these that keeps
/// the kerb clear of the corner's sidewalk slab; and the flare's radius where a street's
/// kerb, parkway and walk open out into a turning circle's.
const KERB_RETURNS: [f64; 6] = [2.0, 1.7, 1.4, 1.1, 0.8, 0.5];
const BULB_FLARE: f64 = 3.0;

/// The walk along `street` on its left (1) or right (-1): its near and far edges' distance
/// (m) from the street's line; on Senda Parkway's south side its wide path's.
fn walk_band(street: Street, side: f64) -> (f64, f64) {
    let half = street.half();
    if street == Street::SendaParkway && side < 0.0 {
        (half + WAY_PARKWAY, half + WAY_PARKWAY + WAY_PATH)
    } else {
        (half + PARKWAY, half + PARKWAY + SIDEWALK)
    }
}

/// A street corner where a side street leaves the street it starts from: the kerbs turn
/// it round a kerb return, the main street's walk runs on through one square corner slab
/// to meet the side street's walk, which starts at it.
struct Corner {
    main: Street,
    side_street: Street,
    /// The main street's side the side street leaves on, and the side street's side the
    /// corner lies on (1 left, -1 right).
    main_side: f64,
    side: f64,
    /// Where the side street's line starts, on the main street's asphalt.
    junction: DVec2,
    /// The kerb return's middle, and where along each street it meets its straight kerb.
    centre: DVec2,
    main_at: f64,
    side_at: f64,
    main_line: Line,
    /// The edges of the walks the corner slab lies between, as the walks' ribbons run.
    main_inner: Line,
    main_outer: Line,
    side_inner: Line,
    side_outer: Line,
}

/// Every street corner in the level.
fn corners() -> Vec<Corner> {
    corner_plans().iter().map(CornerPlan::corner).collect()
}

/// A corner's placing, found once: the rest follows from it.
#[derive(Clone, Copy)]
struct CornerPlan {
    main: Street,
    side_street: Street,
    main_side: f64,
    side: f64,
    centre: DVec2,
    radius: f64,
}

impl CornerPlan {
    fn corner(&self) -> Corner {
        let (main_line, side_line) = (self.main.line(), self.side_street.line());
        let (main_near, main_far) = walk_band(self.main, self.main_side);
        let (side_near, side_far) = walk_band(self.side_street, self.side);
        Corner {
            main: self.main,
            side_street: self.side_street,
            main_side: self.main_side,
            side: self.side,
            junction: side_line.0[0],
            centre: self.centre,
            main_at: main_line.along(self.centre),
            side_at: side_line.along(self.centre),
            main_inner: ribbon_edge(&main_line.0, self.main_side * main_near),
            main_outer: ribbon_edge(&main_line.0, self.main_side * main_far),
            side_inner: ribbon_edge(&side_line.0, self.side * side_near),
            side_outer: ribbon_edge(&side_line.0, self.side * side_far),
            main_line,
        }
    }
}

fn corner_plans() -> Vec<CornerPlan> {
    let mut plans = Vec::new();
    for side_street in Street::ALL {
        for main in Street::ALL {
            let (a, b) = (main.line(), side_street.line());
            if main == side_street || a.distance(b.0[0]) > main.half() {
                continue;
            }
            let main_side = a.signed_distance(b.at(main.half() + 2.0).0).signum();
            for side in [1.0, -1.0] {
                let plan = KERB_RETURNS
                    .iter()
                    .map(|&radius| CornerPlan {
                        main,
                        side_street,
                        main_side,
                        side,
                        centre: kerb_return_centre(main, side_street, (main_side, side), radius),
                        radius,
                    })
                    .find(CornerPlan::clear_of_walks);
                plans.extend(plan);
            }
        }
    }
    plans
}

/// The middle of a kerb return of `radius` between the kerbs of `main` on its
/// `main_side` and of `side_street` on its `side`: that far out from both, found on the
/// streets' spans there.
fn kerb_return_centre(
    main: Street,
    side_street: Street,
    (main_side, side): (f64, f64),
    radius: f64,
) -> DVec2 {
    let (a, b) = (main.line(), side_street.line());
    let (mut sa, mut sb) = (a.along(b.0[0]), main.half() + radius);
    let mut centre = b.0[0];
    for _ in 0..4 {
        let ((pa, ta), (pb, tb)) = (a.at(sa), b.at(sb));
        let pa = pa + left(ta) * main_side * (main.half() + radius);
        let pb = pb + left(tb) * side * (side_street.half() + radius);
        let cross = |u: DVec2, v: DVec2| u.x * v.y - u.y * v.x;
        centre = pa + ta * (cross(pb - pa, tb) / cross(ta, tb));
        (sa, sb) = (a.along(centre), b.along(centre));
    }
    centre
}

impl CornerPlan {
    /// Whether the kerb return's top keeps off both walks.
    fn clear_of_walks(&self) -> bool {
        let (a, b) = (self.main.line(), self.side_street.line());
        let (main_near, main_far) = walk_band(self.main, self.main_side);
        let (side_near, side_far) = walk_band(self.side_street, self.side);
        let on_walk = |p: DVec2| {
            let (da, db) = (
                self.main_side * a.signed_distance(p),
                self.side * b.signed_distance(p),
            );
            (da > main_near - 0.05 && da < main_far && db > side_near - 0.05)
                || (db > side_near - 0.05 && db < side_far && da > main_far - 0.05)
        };
        let toward_a = -left(a.at(a.along(self.centre)).1) * self.main_side;
        let toward_b = -left(b.at(b.along(self.centre)).1) * self.side;
        let reach = self.radius - KERB_WIDTH - 0.03;
        (0..=8).all(|k| {
            let toward = toward_a.lerp(toward_b, f64::from(k) / 8.0).normalize();
            !on_walk(self.centre + toward * reach)
        })
    }
}

/// Where a street's kerb, parkway and walk open out into its turning circle's on one
/// side: round a flare of [`BULB_FLARE`] about `centre` that meets the street's kerb `at`
/// metres along it and the circle's at `angle` (radians) round it.
struct Flare {
    street: Street,
    side: f64,
    centre: DVec2,
    at: f64,
    angle: f64,
    /// The street's heading where the flare leaves it.
    toward: DVec2,
}

/// Every turning circle's flares, its street's left one first.
fn flares() -> Vec<Flare> {
    let mut flares = Vec::new();
    for street in Street::ALL {
        let Some(middle) = street.cul_de_sac() else {
            continue;
        };
        let line = street.line();
        let reach = CUL_DE_SAC_RADIUS + BULB_FLARE;
        for side in [1.0, -1.0] {
            let mut at = line.along(middle) - reach;
            let mut centre = middle;
            for _ in 0..4 {
                let (point, along) = line.at(at);
                let edge = point + left(along) * side * (street.half() + BULB_FLARE);
                // Along the street's kerb line that far out, to where it lies the circle's
                // radius and the flare's from the circle's middle, short of it.
                let q = edge - middle;
                let b = q.dot(along);
                let x = -b - (b * b - q.length_squared() + reach * reach).max(0.0).sqrt();
                centre = edge + along * x;
                at = line.along(centre);
            }
            let out = centre - middle;
            flares.push(Flare {
                street,
                side,
                centre,
                at,
                angle: out.y.atan2(out.x),
                toward: line.at(at).1,
            });
        }
    }
    flares
}

/// How far `p` lies inside the stretch of `street`'s walk on `side` that is drawn
/// (negative where it gives way): to the corner slab of a side street leaving it there,
/// to the walk of the street a side street leaves, and to a turning circle's flare.
fn walk_keep(street: Street, side: f64, corners: &[Corner], flares: &[Flare], p: DVec2) -> f64 {
    // Far from where it gives way, by more than any one ribbon piece.
    let mut keep: f64 = 1e3;
    let mut slab: f64 = f64::NEG_INFINITY;
    for corner in corners {
        if corner.junction.distance(p) > CORNER_REACH {
            continue;
        }
        if corner.main == street && corner.main_side == side {
            slab = slab.max(corner.side * corner.side_inner.signed_distance(p));
        }
        if corner.side_street == street {
            keep = keep.min(corner.main_side * corner.main_outer.signed_distance(p));
        }
    }
    for flare in flares {
        if flare.street == street && flare.side == side && flare.centre.distance(p) < CORNER_REACH {
            keep = keep.min((flare.centre - p).dot(flare.toward));
        }
    }
    if slab.is_finite() {
        keep = keep.min(slab);
    }
    keep
}

/// Whether `p` on `street`'s walk on `side` lies on a corner slab (or past it, where the
/// walk gives way), where the walk's own score lines would cross the slab.
fn on_corner_slab(street: Street, side: f64, corners: &[Corner], p: DVec2) -> bool {
    corners
        .iter()
        .filter(|corner| {
            corner.main == street
                && corner.main_side == side
                && corner.junction.distance(p) < CORNER_REACH
        })
        .map(|corner| {
            let (near, far) = walk_band(corner.side_street, corner.side);
            corner.side * corner.side_inner.signed_distance(p) - (far - near)
        })
        .reduce(f64::max)
        .is_some_and(|past| past < 0.05)
}

/// No asphalt patch nearer (m) a junction or a turning circle's rim than this.
const PATCH_CORNER_ROOM: f64 = 9.0;
/// The pieces a kerb return, a flare and a driveway's apron are laid in.
const CORNER_PIECES: usize = 8;
const APRON_PIECES: usize = 4;

/// The stretches (metres along it) of `street`'s straight kerb on `side`: from where a
/// side street's leaves its kerb return to where a turning circle's flare takes over,
/// broken across the mouths of the side streets that leave it.
fn kerb_spans(
    street: Street,
    side: f64,
    length: f64,
    corners: &[Corner],
    flares: &[Flare],
) -> Vec<(f64, f64)> {
    let mut from: f64 = 0.0;
    let mut to = length;
    for corner in corners {
        if corner.side_street == street && corner.side == side {
            from = from.max(corner.side_at);
        }
    }
    for flare in flares {
        if flare.street == street && flare.side == side {
            to = to.min(flare.at);
        }
    }
    // A side street's mouth: from its one kerb return to the other.
    let mut mouths: Vec<(f64, f64)> = Vec::new();
    for side_street in Street::ALL {
        let ends = corners
            .iter()
            .filter(|corner| {
                corner.main == street
                    && corner.main_side == side
                    && corner.side_street == side_street
            })
            .map(|corner| corner.main_at);
        if let Some((a, b)) = ends.fold(None, |span: Option<(f64, f64)>, s| {
            Some(span.map_or((s, s), |(a, b)| (a.min(s), b.max(s))))
        }) {
            mouths.push((a, b));
        }
    }
    mouths.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut spans = Vec::new();
    let mut start = from;
    for (a, b) in mouths {
        if a.min(to) > start {
            spans.push((start, a.min(to)));
        }
        start = start.max(b);
    }
    if to > start {
        spans.push((start, to));
    }
    spans.retain(|(a, b)| b - a > 1e-3);
    spans
}

/// The points round a corner about `centre` from `from` to `to`: turned and stretched
/// smoothly between them, so the curve meets both exactly.
fn round_corner(centre: DVec2, from: DVec2, to: DVec2, share: f64) -> DVec2 {
    let angle = |v: DVec2| v.y.atan2(v.x);
    let start = angle(from - centre);
    let sweep = (angle(to - centre) - start + std::f64::consts::PI)
        .rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI;
    let (radius, reach) = (
        from.distance(centre),
        to.distance(centre) - from.distance(centre),
    );
    centre + DVec2::from_angle(start + sweep * share) * (radius + reach * share)
}

/// [`round_corner`]'s points in `pieces` even steps, both ends included.
fn round_corner_points(centre: DVec2, from: DVec2, to: DVec2, pieces: usize) -> Vec<DVec2> {
    (0..=pieces)
        .map(|k| round_corner(centre, from, to, k as f64 / pieces as f64))
        .collect()
}

/// Where two lines, through `a` along `ta` and through `b` along `tb`, cross.
fn meet(a: DVec2, ta: DVec2, b: DVec2, tb: DVec2) -> Option<DVec2> {
    let cross = |u: DVec2, v: DVec2| u.x * v.y - u.y * v.x;
    let turn = cross(ta, tb);
    (turn.abs() > 1e-6).then(|| a + ta * (cross(b - a, tb) / turn))
}

/// A thin score line across a walk from `a` to `b`, at the joints' height.
fn joint_between(surface: &mut Surface, a: DVec2, b: DVec2) {
    let half = left((b - a).normalize()) * (CONCRETE_JOINT_WIDTH / 2.0);
    surface.polygon(
        &[a - half, b - half, b + half, a + half],
        WALK_TOP + JOINT_LIFT,
    );
}

/// A street corner: the kerb return with its gutter pan, the asphalt filling the corner
/// out to it from where the two kerbs' lines would meet, and the score lines that make
/// the main street's walk there one square slab, across it where the side street's walk
/// would run on and where that walk starts.
fn street_corner(group: &mut Node, corner: &Corner) {
    let from = street_kerb_foot(corner.main, corner.main_side, corner.main_at).0;
    let to = street_kerb_foot(corner.side_street, corner.side, corner.side_at).0;
    let side_line = corner.side_street.line();
    let corner_point = meet(
        from,
        corner.main_line.at(corner.main_at).1,
        to,
        side_line.at(corner.side_at).1,
    );
    if let Some(corner_point) = corner_point {
        let arc = round_corner_points(corner.centre, from, to, CORNER_PIECES);
        let mut fill = Surface::default();
        for pair in arc.windows(2) {
            fill.polygon(&[corner_point, pair[0], pair[1]], STREET_TOP);
        }
        group.children.push(flat(fill.mesh(), ASPHALT));
    }
    let (main_side, side) = (corner.main_side, corner.side);
    // Across the main street's walk where the side street's walk would run on, and
    // along its outer edge where that walk starts.
    let across_main = [&corner.main_inner, &corner.main_outer].map(|main| {
        crossing(
            &corner.side_outer.0,
            &|p: DVec2| main_side * main.signed_distance(p),
            corner.junction,
            CORNER_REACH,
        )
    });
    let mut joints = Surface::default();
    if let [Some(a), Some(b)] = across_main {
        joint_between(&mut joints, a, b);
    }
    let along_main = [&corner.side_inner, &corner.side_outer].map(|edge| {
        crossing(
            &corner.main_outer.0,
            &|p: DVec2| side * edge.signed_distance(p),
            corner.junction,
            CORNER_REACH,
        )
    });
    if let [Some(a), Some(b)] = along_main {
        joint_between(&mut joints, a, b);
    }
    group.children.push(flat(joints.mesh(), CONCRETE_JOINT));
}

/// A turning circle's flare on one side of its street: the kerb and gutter opening out
/// round it from the street's to the circle's, the asphalt filling it out to them, and
/// the walk turning with them from the street's onto the circle's.
fn circle_flare(group: &mut Node, flare: &Flare, (corners, flares): (&[Corner], &[Flare])) {
    let (street, side) = (flare.street, flare.side);
    let Some(middle) = street.cul_de_sac() else {
        return;
    };
    let from = street_kerb_foot(street, side, flare.at).0;
    let out = DVec2::from_angle(flare.angle);
    let to = middle + out * CUL_DE_SAC_RADIUS;
    // The asphalt from where the street's kerb line runs into the circle.
    let q = from - middle;
    let b = q.dot(flare.toward);
    let x = -b
        - (b * b - q.length_squared() + CUL_DE_SAC_RADIUS * CUL_DE_SAC_RADIUS)
            .max(0.0)
            .sqrt();
    let corner_point = from + flare.toward * x;
    let arc = round_corner_points(flare.centre, from, to, CORNER_PIECES);
    let mut fill = Surface::default();
    for pair in arc.windows(2) {
        fill.polygon(&[corner_point, pair[0], pair[1]], STREET_TOP);
    }
    group.children.push(flat(fill.mesh(), ASPHALT));
    // The walk, from where the street's ends to the circle's.
    let line = street.line();
    let (near, far) = walk_band(street, side);
    let keep = |p: DVec2| walk_keep(street, side, corners, flares, p);
    let ends = [near, far].map(|offset| {
        crossing(
            &ribbon_edge(&line.0, side * offset).0,
            &keep,
            flare.centre,
            CORNER_REACH,
        )
    });
    let [Some(near_end), Some(far_end)] = ends else {
        return;
    };
    let ring = |offset: f64| middle + out * (CUL_DE_SAC_RADIUS + offset - street.half());
    let inner = round_corner_points(flare.centre, near_end, ring(near), CORNER_PIECES);
    let outer = round_corner_points(flare.centre, far_end, ring(far), CORNER_PIECES);
    let mut walk = Surface::default();
    for k in 0..CORNER_PIECES {
        walk.polygon(&[inner[k], inner[k + 1], outer[k + 1], outer[k]], WALK_TOP);
    }
    group.children.push(flat(walk.mesh(), SIDEWALK_CONCRETE));
}

/// A kerb's face: an upright strip along `line` at `at` metres to its left, from the
/// asphalt's top to the kerb's, facing toward its left (`facing` +1) or right (-1), in the
/// pieces [`ribbon`] cuts the kerb from `kerb.0` to `kerb.1` metres into and leaving out
/// the same ones, so face and top end together.
fn kerb_face(
    line: &[DVec2],
    at: f64,
    kerb: (f64, f64),
    facing: f64,
    skip: &dyn Fn(DVec2) -> bool,
) -> Mesh {
    let n = line.len();
    let normal = |i: usize| {
        let along = (line[(i + 1).min(n - 1)] - line[i.saturating_sub(1)]).normalize();
        left(along)
    };
    let mut wall = Wall::default();
    for i in 0..n - 1 {
        let (a, b) = (line[i], line[i + 1]);
        let (na, nb) = (normal(i), normal(i + 1));
        let pieces = (a.distance(b) / 1.0).ceil().max(1.0) as usize;
        for k in 0..pieces {
            let (t0, t1) = (k as f64 / pieces as f64, (k + 1) as f64 / pieces as f64);
            let point = |t: f64, offset: f64| a.lerp(b, t) + na.lerp(nb, t).normalize() * offset;
            if skip((point(t0, kerb.0) + point(t1, kerb.1)) / 2.0) {
                continue;
            }
            let out = na.lerp(nb, (t0 + t1) / 2.0).normalize() * facing;
            wall.quad(point(t0, at), point(t1, at), (STREET_TOP, KERB_HEIGHT), out);
        }
    }
    wall.mesh()
}

/// Upright quads gathered into one flat-shaded mesh, world-aligned uvs like the paving's.
#[derive(Default)]
struct Wall {
    positions: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
}

impl Wall {
    /// The quad from `a` to `b` on the ground plan, `y0` to `y1` up, facing `out`.
    fn quad(&mut self, a: DVec2, b: DVec2, (y0, y1): (f64, f64), out: DVec2) {
        let corners = [(a, y0), (b, y0), (b, y1), (a, y1)];
        // Counter-clockwise seen from its front: (b - a) × up points along (-dz, dx).
        let along = b - a;
        let order = if DVec2::new(-along.y, along.x).dot(out) > 0.0 {
            [0, 1, 2, 0, 2, 3]
        } else {
            [0, 2, 1, 0, 3, 2]
        };
        for index in order {
            let (p, y) = corners[index];
            self.positions.extend([p.x, y, p.y]);
            self.normals.extend([out.x, 0.0, out.y]);
            self.uvs.extend([(p.x + p.y) / 4.0, y / 4.0]);
        }
    }

    fn mesh(self) -> Mesh {
        Mesh::from_f64(&self.positions, &self.normals, &self.uvs, None)
    }
}

/// The score lines across a walk along `line` between `from` and `to` metres to its
/// left: a thin dark joint every [`WALK_SCORE`] metres, at `y`, but where `skip` holds.
fn score_lines(
    line: &[DVec2],
    (from, to): (f64, f64),
    y: f64,
    skip: &dyn Fn(DVec2) -> bool,
) -> Mesh {
    let trace = Line(line.to_vec());
    let mut positions: Vec<f64> = Vec::new();
    let mut s = WALK_SCORE;
    while s < trace.length() {
        let (point, along) = trace.at(s);
        s += WALK_SCORE;
        let across = left(along);
        if skip(point + across * (from + to) / 2.0) {
            continue;
        }
        let half = along * (CONCRETE_JOINT_WIDTH / 2.0);
        let (a, b) = (point + across * from, point + across * to);
        let quad = [a - half, a + half, b + half, b - half];
        let order = if to > from {
            [0, 1, 2, 0, 2, 3]
        } else {
            [0, 2, 1, 0, 3, 2]
        };
        for index in order {
            positions.extend([quad[index].x, y, quad[index].y]);
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

/// A residential street's wear: the paving joint a little off its middle, where the two
/// passes of the paver met (sealed a shade darker), and a few long, narrow patches of
/// newer asphalt over old utility trenches beside one gutter or the other, one every so
/// far along it, but none where `by_a_corner` says it would lie by a junction or a
/// turning circle; the seam stops where `seam_keep` turns negative, at the kerb line of
/// the street it leaves. Senda Parkway, striped, keeps neither.
fn asphalt_wear(
    group: &mut Node,
    street: Street,
    line: &[DVec2],
    by_a_corner: &dyn Fn(DVec2) -> bool,
    seam_keep: &dyn Fn(DVec2) -> f64,
) {
    if street == Street::SendaParkway {
        return;
    }
    let nothing = |_: DVec2| false;
    let (offset, width) = ASPHALT_SEAM;
    group.children.push(flat(
        ribbon_cut(
            line,
            offset - width / 2.0,
            offset + width / 2.0,
            STREET_TOP + SEAM_LIFT,
            &nothing,
            seam_keep,
        ),
        ASPHALT_SEALED,
    ));
    let trace = Line(line.to_vec());
    let mut s = ASPHALT_PATCH_STEP * 0.4;
    let mut k = 0;
    while s < trace.length() {
        let seed = s * 1.31 + k as f64 * 7.0;
        let (length, width) = (
            ASPHALT_PATCH.0 * (0.6 + 0.8 * hash(seed)),
            ASPHALT_PATCH.1 * (0.6 + 0.8 * hash(seed + 0.4)),
        );
        // Along the street's own curve, just clear of a gutter pan.
        let patch = cut_line(&trace, s, (s + length).min(trace.length()));
        let side = if hash(seed + 0.9) < 0.5 { -1.0 } else { 1.0 };
        let middle = side * (street.half() - GUTTER.0 - 0.1 - width / 2.0);
        let near_corner = (0..=8).any(|k| {
            let (point, along) = trace.at(s + length * f64::from(k) / 8.0);
            by_a_corner(point + left(along) * middle)
        });
        if !near_corner {
            group.children.push(flat(
                ribbon(
                    &patch,
                    middle - width / 2.0,
                    middle + width / 2.0,
                    STREET_TOP + PATCH_LIFT,
                    &nothing,
                ),
                ASPHALT_PATCH_TONE,
            ));
        }
        s += ASPHALT_PATCH_STEP * (0.7 + 0.6 * hash(seed + 1.7));
        k += 1;
    }
}

/// A painted, shadow-receiving flat piece.
fn flat(mesh: Mesh, color: u32) -> Node {
    let mut node = shadow_receiver(Arc::new(mesh), paint(color));
    paint_mesh(&mut node);
    node
}

/// The paseos' ways out: each from the west park's nearest walk through its paseo to the
/// back of Neblina's sidewalk, as a tank drives it.
fn paseo_ways() -> Vec<Line> {
    let park_walks: Vec<DVec2> = west_park()
        .map(|park| {
            std::iter::once(park.oval_line())
                .chain(park.walks.iter().cloned())
                .flatten()
                .collect()
        })
        .unwrap_or_default();
    paseos(&senda_houses())
        .into_iter()
        .map(|paseo| {
            let mut line = Vec::new();
            let nearest = park_walks
                .iter()
                .copied()
                .min_by(|a, b| a.distance(paseo.field).total_cmp(&b.distance(paseo.field)));
            if let Some(start) = nearest.filter(|start| start.distance(paseo.field) > 0.5) {
                line.push(start);
            }
            line.extend([paseo.field, paseo.sidewalk]);
            Line(line)
        })
        .collect()
}

/// Whether `p` keeps `room` (m) clear of every one of `ways` (a paseo's width each).
fn off_paseos(ways: &[Line], p: DVec2, room: f64) -> bool {
    ways.iter()
        .all(|way| way.distance(p) > PASEO_WIDTH / 2.0 + room)
}

/// The paseos: a concrete walk down each one's middle, scored every so often, from the
/// back of Neblina's sidewalk out past the back walls and on to the west park's walk; a
/// band of decomposed granite either side; and against each wall a strip of bark mulch
/// planted with low clipped shrubs, a few in flower. Nothing in it stands taller than a
/// tank's tracks: it is a way out.
fn paseo_grounds() -> Node {
    let mut group = Node::group("senda-paseos");
    let off = |p: DVec2| is_senda_paved(p.x, p.y);
    let (walk, granite, strip) = PASEO_BANDS;
    for way in paseo_ways() {
        let n = way.0.len();
        let inner: Vec<DVec2> = way.0[n - 2..].to_vec();
        if n > 2 {
            // The walk from the park runs on under the paseo's walk, cut where it would
            // show past that walk's sides, so the two join along the paseo's end; it
            // starts under the park's walk.
            let (field, sidewalk) = (inner[0], inner[1]);
            let into = (sidewalk - field).normalize();
            let mut link: Vec<DVec2> = way.0[..n - 1].to_vec();
            let toward = (field - link[link.len() - 2]).normalize();
            link.push(field + toward * (walk + WEST_PARK_WALK));
            let joined = |p: DVec2| {
                let offset = p - field;
                (-offset.dot(into)).max(walk - offset.dot(left(into)).abs())
            };
            group.children.push(flat(
                ribbon_cut(
                    &link,
                    -WEST_PARK_WALK / 2.0,
                    WEST_PARK_WALK / 2.0,
                    PASEO_LINK_TOP,
                    &off,
                    &joined,
                ),
                SIDEWALK_CONCRETE,
            ));
        }
        group.children.push(flat(
            ribbon(&inner, -walk, walk, WALK_TOP - 0.01, &off),
            SIDEWALK_CONCRETE,
        ));
        for side in [-1.0, 1.0] {
            let (a, b) = (side * walk, side * granite);
            group.children.push(flat(
                ribbon(&inner, a.min(b), a.max(b), 0.03, &off),
                PASEO_GRANITE,
            ));
            let (a, b) = (side * granite, side * strip);
            group.children.push(flat(
                ribbon(&inner, a.min(b), a.max(b), 0.03, &off),
                BED_MULCH,
            ));
        }
        let line = Line(inner);
        let length = line.length();
        let mut s = PASEO_SCORE;
        while s < length - 0.3 {
            let (at, along) = line.at(s);
            if !off(at) {
                let joint = rotated(
                    box_part(walk * 2.0 - 0.06, 0.004, 0.025, PASEO_JOINT, 0.0),
                    0.0,
                    along.x.atan2(along.y) + FRAC_PI_2,
                    0.0,
                );
                put(&mut group, joint, at.x, WALK_TOP - 0.008, at.y);
            }
            s += PASEO_SCORE;
        }
        let mut s = PASEO_SHRUB_STEP / 2.0;
        let mut k = 0;
        while s < length {
            let (point, along) = line.at(s);
            s += PASEO_SHRUB_STEP;
            for side in [-1.0, 1.0] {
                k += 1;
                let seed = s * 3.1 + side * 7.0;
                let at = point + left(along) * side * (granite + strip) / 2.0;
                if off(at) {
                    continue;
                }
                let (least, most) = PASEO_SHRUB;
                let size = least + (most - least) * hash(seed);
                let blossom = (k % 5 == 2).then_some(FRONT_BLOSSOM[k % FRONT_BLOSSOM.len()]);
                rounded_shrub(
                    &mut group,
                    (at, 0.03),
                    (size, size * 1.1),
                    ShrubForm::Mound,
                    PLANTED_LEAVES[k % PLANTED_LEAVES.len()],
                    blossom,
                    seed,
                );
            }
        }
    }
    batch(&mut group);
    group
}

/// Every paved surface, batched into one draw per material.
fn paving() -> Node {
    let mut group = Node::group("senda-paving");
    let lines = Lines::new();
    let (corners, flares) = (corners(), flares());
    // Every kerb with its gutter pan, the corners' and circles' too, in one piece each,
    // lowered where a driveway crosses it.
    let runs = kerb_runs(&corners, &flares);
    let mut drops: Vec<Vec<(f64, f64)>> = runs.iter().map(|_| Vec::new()).collect();
    for (k, lot) in lines.lots.iter().enumerate() {
        let Some(hits) = front_yard(lot, k).and_then(|yard| driveway_kerb(lot, &yard, &runs))
        else {
            continue;
        };
        for (run, drop) in kerb_drops(&runs, [hits[0], hits[APRON_PIECES]]) {
            drops[run].push(drop);
        }
    }
    let (mut kerbs, mut pans) = (Surface::default(), Surface::default());
    for (run, dropped) in runs.iter().zip(&drops) {
        kerb_run(&run.points(dropped), &mut kerbs, &mut pans);
    }
    group.children.push(flat(kerbs.mesh(), KERB));
    group.children.push(flat(pans.mesh(), GUTTER.1));
    let bulbs: Vec<DVec2> = Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .collect();
    let nothing = |_: DVec2| false;
    for street in Street::ALL {
        let trace = street.line();
        let (line, half) = (&trace.0, street.half());
        // The asphalt runs on into the junctions and the turning circles; the kerbs turn
        // their corners and flare into the circles, and the walks give way to each other
        // at a corner slab.
        group.children.push(flat(
            ribbon(line, -half, half, STREET_TOP, &nothing),
            ASPHALT,
        ));
        for side in [-1.0, 1.0] {
            let (near, far) = walk_band(street, side);
            let keep = |p: DVec2| walk_keep(street, side, &corners, &flares, p);
            let walk = ribbon_cut(line, side * near, side * far, WALK_TOP, &nothing, &keep);
            group.children.push(flat(walk, SIDEWALK_CONCRETE));
            let joints = score_lines(
                line,
                (side * near, side * far),
                WALK_TOP + JOINT_LIFT,
                &|p: DVec2| keep(p) < 0.05 || on_corner_slab(street, side, &corners, p),
            );
            group.children.push(flat(joints, CONCRETE_JOINT));
        }
        // No patch by a corner or a circle, where a short one read as a hole; a side
        // street's seam stops at the kerb line of the street it leaves.
        let by_a_corner = |p: DVec2| {
            corners
                .iter()
                .any(|corner| corner.junction.distance(p) < PATCH_CORNER_ROOM)
                || bulbs
                    .iter()
                    .any(|middle| middle.distance(p) < CUL_DE_SAC_RADIUS + PATCH_CORNER_ROOM)
        };
        let seam_keep = |p: DVec2| {
            corners
                .iter()
                .filter(|corner| {
                    corner.side_street == street && corner.junction.distance(p) < CORNER_REACH
                })
                .map(|corner| {
                    corner.main_side * corner.main_line.signed_distance(p) - corner.main.half()
                })
                .fold(1e3, f64::min)
        };
        asphalt_wear(&mut group, street, line, &by_a_corner, &seam_keep);
        if street == Street::SendaParkway {
            senda_parkway(&mut group, line);
        }
    }
    for corner in &corners {
        street_corner(&mut group, corner);
    }
    for flare in &flares {
        circle_flare(&mut group, flare, (&corners, &flares));
    }
    let lawn = lawn_material();
    for street in Street::ALL {
        if let Some(middle) = street.cul_de_sac() {
            cul_de_sac(&mut group, street, middle, &lawn, &flares);
        }
    }
    neblina_centre_line(&mut group);
    splitter_island(&mut group);
    greenbelt_path(&mut group, &lines);
    trails(&mut group, &lines);
    yard_grounds(&mut group, &lines.lots);
    front_lawns(&mut group, &lines.lots, &lawn, &runs);
    bend_park_grounds(&mut group, &lines, &lawn);
    west_park_grounds(&mut group, &lines);
    crest_basins(&mut group, &lines);
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
        let paint = ribbon(&line.0, near, far, STREET_TOP + PAINT_LIFT, &skip);
        group.children.push(flat(paint, CENTRE_LINE_PAINT));
    }
}

/// The kerbed splitter island in Neblina's mouth, mulched as the median is: round at its
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
    // The gutter pan round its foot, its kerb and its mulch, each a little inside the
    // last.
    for (grow, y, color) in [
        (
            KERB_WIDTH + ISLAND_GUTTER,
            STREET_TOP + GUTTER_LIFT,
            GUTTER.1,
        ),
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
    // The kerb's face round its outline, across its blunt nose and tail too.
    let outline = |t: f64, sign: f64| {
        let (point, along) = neblina.at(from + t);
        point + left(along) * sign * (half(t) + KERB_WIDTH)
    };
    let mut face = Wall::default();
    let length = to - from;
    for k in 0..pieces {
        let t0 = k as f64 * ISLAND_PIECE;
        let t1 = (t0 + ISLAND_PIECE).min(length);
        for sign in [-1.0, 1.0] {
            let (a, b) = (outline(t0, sign), outline(t1, sign));
            let along = b - a;
            let out = DVec2::new(-along.y, along.x).normalize();
            let axis = neblina.at(from + (t0 + t1) / 2.0).0;
            let out = if out.dot((a + b) / 2.0 - axis) > 0.0 {
                out
            } else {
                -out
            };
            face.quad(a, b, (STREET_TOP, KERB_HEIGHT), out);
        }
    }
    for (t, outward) in [(0.0, -1.0), (length, 1.0)] {
        let (_, along) = neblina.at(from + t);
        face.quad(
            outline(t, -1.0),
            outline(t, 1.0),
            (STREET_TOP, KERB_HEIGHT),
            along * outward,
        );
    }
    group.children.push(flat(face.mesh(), KERB));
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
            let (inner, outer) = (side * MEDIAN_HALF, side * reach);
            let kerb = ribbon(&span, inner, outer, KERB_HEIGHT, &nothing);
            group.children.push(flat(kerb, KERB));
            let face = kerb_face(&span, outer, (inner, outer), side, &nothing);
            group.children.push(flat(face, KERB));
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
            let side = left(out);
            let point = |k: usize| {
                let angle = std::f64::consts::PI * k as f64 / NOSE_SEGMENTS as f64;
                centre + (side * angle.cos() + out * angle.sin()) * reach
            };
            let mut face = Wall::default();
            for k in 0..NOSE_SEGMENTS {
                let (a, b) = (point(k), point(k + 1));
                let outward = ((a + b) / 2.0 - centre).normalize();
                face.quad(a, b, (STREET_TOP, KERB_HEIGHT), outward);
            }
            group.children.push(flat(face.mesh(), KERB));
        }
    }
    let half = Street::SendaParkway.half();
    let middle = half - BIKE_LANE;
    let (a, b) = (middle - LANE_LINE / 2.0, middle + LANE_LINE / 2.0);
    for side in [-1.0, 1.0] {
        group.children.push(flat(
            ribbon(line, side * a, side * b, STREET_TOP + PAINT_LIFT, &nothing),
            LANE_PAINT,
        ));
    }
}

/// The greenbelt's path: a narrow walk of packed bark mulch down its lane's middle, from
/// Neblina's sidewalk to the hillside.
fn greenbelt_path(group: &mut Node, lines: &Lines) {
    let line = greenbelt_lane();
    if line.len() < 2 {
        return;
    }
    let half = PATH_WIDTH / 2.0;
    let off_ground = |p: DVec2| is_senda_paved(p.x, p.y) || lines.lot(p).0 > -0.1;
    group.children.push(flat(
        ribbon(&line, -half, half, 0.035, &off_ground),
        PATH_MULCH,
    ));
}

/// A smooth step from 0 at `x` 0 to 1 at 1.
fn rise(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the greenbelt's planting leaves open: its walk and the spawn pads and pickups,
/// each with its room; and the line its trees follow.
struct Thicket {
    path: Line,
    creek: Line,
    clear: Vec<(DVec2, f64)>,
}

impl Thicket {
    fn new(lines: &Lines) -> Self {
        let path = Line(greenbelt_lane());
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

/// How much of the greenbelt's groundcover stands at `p`, from 0 to 1, rising from the
/// lots' walls, the walk, the spots it keeps clear and the end of the greenbelt
/// short of the hillside, and none in its planted mouth; and how far into the lane's
/// middle it lies, from 0 by the walls to 1, where the thicket stands tallest.
fn mat_weight(p: DVec2, lines: &Lines, thicket: &Thicket) -> (f64, f64) {
    let zone = zones(p, lines);
    let off_walls = -lines.lot(p).0;
    let mut w = (zone.greenbelt - zone.mouth).max(0.0)
        * rise((off_walls - MAT_WALL_ROOM) / MAT_RISE)
        * rise((HILLSIDE_FROM + 4.0 - p.x) / 6.0);
    if thicket.path.0.len() > 1 {
        w *= rise((thicket.path.distance(p) - MAT_LANE_CLEAR) / MAT_RISE);
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

/// The line the greenbelt's trees follow: meandering down the lane between the back
/// walls, kept off the walls and the walk; empty where there is no room.
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

/// The greenbelt: the narrow planted slope between the back walls, low groundcover with
/// shrubs in drifts and a few small trees down its middle, and the view fence across its
/// mouth.
fn greenbelt_scrub() -> Node {
    let mut group = Node::group("senda-greenbelt");
    let lines = Lines::new();
    let thicket = Thicket::new(&lines);
    if let Some(mat) = greenbelt_mat(&lines, &thicket) {
        group.children.push(mat);
    }
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
                let drift = 0.5
                    + 0.3 * (at.x * 0.31 + at.y * 0.47).sin() * (at.y * 0.29 - at.x * 0.17).cos()
                    + 0.2 * (at.x * 0.13 - at.y * 0.21 + 0.8).sin();
                let (least, most) = MAT_BUSH;
                let size = least + (most - least) * hash(seed + 1.1) * (0.5 + 0.5 * middle);
                let off_lane =
                    thicket.path.0.len() < 2 || thicket.path.distance(at) > MAT_LANE_CLEAR + size;
                if w > 0.85 && drift > MAT_BUSH_DRIFT && hash(seed + 0.8) < 0.75 && off_lane {
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
            && (thicket.path.0.len() < 2 || thicket.path.distance(middle) > LANE_CLEAR + 1.2)
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
                && (thicket.path.0.len() < 2 || thicket.path.distance(at) > LANE_CLEAR + 0.5)
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

/// The few small trees down the greenbelt's middle, pale-barked and open-crowned: one
/// every [`CREEK_TREE_STEP`] along it, where the groundcover is full, clear of the spawn
/// pads, the pickups and the greenbelt's cover trees.
fn creek_trees(group: &mut Node, lines: &Lines, thicket: &Thicket) {
    if thicket.creek.0.len() < 2 {
        return;
    }
    let oaks: Vec<DVec2> = senda_layout::senda_design_layout()
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
        let (least, most) = CREEK_TREE_CROWN;
        let clear = mat_weight(at, lines, thicket).0 > 0.8
            && thicket.path.distance(at) > LANE_CLEAR + most / 2.0
            && oaks.iter().all(|oak| oak.distance(at) > CREEK_TREE_ROOM)
            && thicket
                .clear
                .iter()
                .all(|&(spot, room)| spot.distance(at) > room + 1.0);
        if !clear {
            continue;
        }
        let seed = s * 1.7;
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
        (MAT_STEP, 4.0),
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
/// left out) and its height (m), `tone` its colour from its spot and height, in the
/// ground's material ([`lawn_material`]) at `grain` times its density, the leaves' fine
/// grain. It shades itself: its mounds are low and close.
fn scrub_surface(
    (low, high): (DVec2, DVec2),
    (step, grain): (f64, f32),
    field: &dyn Fn(DVec2) -> (f64, f64),
    tone: &dyn Fn(DVec2, f64) -> [f32; 3],
) -> Option<Node> {
    let (columns, rows) = (
        ((high.x - low.x) / step).ceil() as usize + 1,
        ((high.y - low.y) / step).ceil() as usize + 1,
    );
    let spot = |k: usize| low + DVec2::new((k % columns) as f64, (k / columns) as f64) * step;
    let fields: Vec<(f64, f64)> = (0..rows * columns).map(|k| field(spot(k))).collect();
    // Where the mat thins to nothing it sinks into the ground: there it takes the colour
    // of the ground it sinks into, so its edge shows as the mounds tapering away rather
    // than as flat islands with straight sides.
    let (hills, lines) = (Hills::new(), Lines::new());
    let (sink, fade) = MAT_SINK;
    let color = |p: DVec2, (weight, height): (f64, f64)| {
        let leaves = tone(p, height);
        // Inside the mat (its weight full) its hollows keep their own shade.
        let shown =
            rise((height - sink - PLANTED_LIFT) / fade).max(rise(2.0 * weight - 1.0)) as f32;
        if shown >= 1.0 {
            return leaves;
        }
        let ground = ground_seen(p, &hills, &lines);
        std::array::from_fn(|i| ground[i] + (leaves[i] - ground[i]) * shown)
    };
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
                    positions.extend([p.x, height - sink, p.y]);
                    colors.push(color(p, fields[k]));
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
    let mut node = shadowed(Arc::new(mesh), lawn_material());
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
    paseos: Vec<Line>,
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
            paseos: paseo_ways(),
        }
    }

    /// Whether `p` keeps `room` (m) clear of every paseo's way.
    fn off_paseos(&self, p: DVec2, room: f64) -> bool {
        off_paseos(&self.paseos, p, room)
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
        // Where a walk cuts across it, the mat slopes down to the paving over a couple of
        // metres, as a planted bank ends, rather than standing cut off at its full height.
        for (reach, line) in &self.streets {
            w *= rise((line.distance(p) - reach - 0.3) / OUTER_MAT_TAPER);
        }
        for &pad in &self.pads {
            w *= rise((pad.distance(p) - OUTER_PAD_ROOM) / MAT_RISE);
        }
        for &pickup in &self.pickups {
            w *= rise((pickup.distance(p) - OUTER_PICKUP_ROOM) / MAT_RISE);
        }
        for way in &self.paseos {
            w *= rise((way.distance(p) - PASEO_WIDTH / 2.0) / OUTER_MAT_TAPER);
        }
        // The south slope's scrub stops short of the lane along the level's wall, which
        // follows the row's back walls rather than one line of z.
        let lane = rise((self.inside(p) - SOUTH_LANE) / SOUTH_MAT_EDGE);
        w *= 1.0 - south * (1.0 - lane);
        // Past the west band the park's lawn.
        w *= south + (1.0 - south) * west_band(p, lines);
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
/// rising over this many metres from the walks, the crest basins and the Way's path, with
/// a round shrub standing out of it at this share of the contour rows' spots; below it a
/// strip of bark mulch with the rows' shrubs; and along the foot, this far (m) out from
/// Rumbo's north row's rear fences, a clipped hedge of shrubs this far apart (m), each
/// this big (radius and how tall for its breadth), in dark grey-greens.
const BANK_COVER_DOWN: f64 = 0.86;
const BANK_CARPET_SHRUBS: f64 = 0.25;
/// The row of clipped round shrubs along the downhill side of the bank's terrace walk:
/// one every this many metres, their radius range (m).
const TERRACE_SHRUB_STEP: f64 = 1.6;
const TERRACE_SHRUB: (f64, f64) = (0.42, 0.52);
/// The bank's east end is dry meadow: tan grass from this far east (arena x, m), over
/// this many metres, with a shrub left at this share of its contour rows' spots.
const BANK_MEADOW: (f64, f64) = (16.0, 10.0);
/// Where on the way into the meadow the bank's groundcover carpet ends, and how far that
/// line wanders either way (shares of the meadow's blend).
const BANK_MEADOW_EDGE: (f64, f64) = (0.4, 0.12);
const MEADOW_SHRUBS: f64 = 0.22;
const BANK_MAT_STEP: f64 = 0.45;
const BANK_MAT: (f64, f64) = (0.1, 0.34);
const BANK_MAT_DOME: f64 = 0.7;
const BANK_MAT_EDGE: f64 = 0.35;
/// The carpet's tints over the packed-dirt tile, linear (its orange tan drawn to a
/// grey green): three tones in drifts, darker toward this in its hollows.
const BANK_MAT_TONES: [[f32; 3]; 3] = [[0.31, 0.59, 1.63], [0.27, 0.52, 1.41], [0.35, 0.66, 1.85]];
const BANK_MAT_SHADE: [f32; 3] = [0.14, 0.26, 0.6];
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
    let field = |p: DVec2| {
        let zone = zones(p, &lines);
        if zone.bank_land <= 0.0 || zone.hillside >= 1.0 {
            return (0.0, 0.0);
        }
        // It ends toward the dry meadow along a wandering line, tapering to the ground
        // within a metre or so rather than thinning over ten into ragged islands.
        let (edge, wander) = BANK_MEADOW_EDGE;
        let uneven = 0.6 * (p.x * 1.13 - p.y * 0.71).sin() * (p.y * 1.37 + p.x * 0.41 + 0.6).cos()
            + 0.4 * (p.y * 2.7 + p.x * 1.9 + 2.3).sin();
        let mut w = zone.bank_land
            * (1.0 - zone.hillside)
            * rise((edge + wander * uneven - zone.meadow) / 0.12)
            * rise((BANK_COVER_DOWN - zone.bank_fall) / 0.05)
            * rise((outer.inside(p) + 0.2) / BANK_MAT_EDGE);
        // Clear of every street's paving: Senda Parkway's path at the bank's top, and
        // Neblina where the bank comes down to it at the junction.
        for (reach, line) in &outer.streets {
            w *= rise((line.distance(p) - reach - 0.1) / BANK_MAT_EDGE);
        }
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
    if let Some(mat) = scrub_surface(bounds, (BANK_MAT_STEP, 4.0), &field, &tone) {
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
        if let Some(mat) = scrub_surface(bounds, (OUTER_MAT_STEP, 4.0), &field, &tone) {
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
            && outer.off_paseos(at, crown / 2.0)
    };
    // Behind Neblina's west row, an olive here and there along the rear walls, in ones and
    // twos rather than a belt of crowns.
    for (k, lot) in lines.lots.iter().enumerate() {
        if lot.house.row != Row::NeblinaWest || k % WEST_OLIVE_EVERY != 1 {
            continue;
        }
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let seed = k as f64 * 17.3;
        let at = lot.at(
            u0 + (u1 - u0) * (0.3 + 0.4 * hash(seed)),
            lot.rear_v() - OLIVE_OUT,
        );
        let (least, most) = OLIVE_CROWN;
        let crown = least + (most - least) * hash(seed + 0.9);
        if clear(at, crown) {
            let (low, high) = OLIVE_HEIGHT;
            let height = low + (high - low) * hash(seed + 1.3);
            olive(&mut group, &bush, at, (crown, height), seed);
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
            && (path.0.len() < 2 || path.distance(at) > LANE_CLEAR + 0.4);
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
    let off_way = |p: DVec2| {
        pads.iter().any(|pad| pad.distance(p) < TRAIL_SPAWN_ROOM) || way.distance(p) < way_reach
    };
    let off_pads = |p: DVec2| off_way(p) || at_wall(p);
    let (terrace, spurs) = bank_trails(lines);
    let terrace_line = Line(terrace.clone());
    // A spur runs up to the level's wall, which stands just outside its boundary line.
    let wall_room = |p: DVec2| {
        let room = if inside_boundary(&boundary, p) {
            BANK_SPUR_WALL_ROOM
        } else {
            BOUNDARY_THICKNESS + BANK_SPUR_WALL_ROOM
        };
        wall.distance(p) - room
    };
    // A spur starts under the terrace walk's upper edge, a few millimetres below it, and
    // ends square at the foot of the level's wall.
    let spur_keep = |p: DVec2, up: f64| {
        let past_terrace =
            up * terrace_line.signed_distance(p) - (BANK_WALK / 2.0 - BANK_SPUR_UNDER);
        past_terrace.min(wall_room(p))
    };
    let walks = spurs
        .into_iter()
        .map(|line| (line, BANK_SPUR, BANK_SPUR_TOP))
        .chain((terrace.len() > 1).then_some((terrace, BANK_WALK, BANK_WALK_TOP)));
    for (line, width, top) in walks {
        let half = width / 2.0;
        let up = if terrace_line.0.len() > 1 && width < BANK_WALK {
            terrace_line.signed_distance(line[line.len() - 1]).signum()
        } else {
            0.0
        };
        let keep = |p: DVec2| if up == 0.0 { 1.0 } else { spur_keep(p, up) };
        let skip: &dyn Fn(DVec2) -> bool = if up == 0.0 { &off_pads } else { &off_way };
        group.children.push(flat(
            ribbon_cut(&line, -half, half, top, skip, &keep),
            SIDEWALK_CONCRETE,
        ));
        let (edge, paler) = BANK_WALK_EDGE;
        for side in [-1.0, 1.0] {
            let (a, b) = (side * half, side * (half - edge));
            let lift = top + BANK_WALK_EDGE_LIFT;
            group.children.push(flat(
                ribbon_cut(&line, a.min(b), a.max(b), lift, skip, &keep),
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
                // Across the meadow the spur climbs slantwise, as the trail there does: one
                // leg, since the level's wall cuts it before a switchback could turn.
                if at.x > BANK_MEADOW.0 {
                    let across = left((path - at).normalize());
                    drains.push(vec![at, path + across * BANK_SWITCHBACK]);
                } else {
                    drains.push(vec![at, path]);
                }
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

/// The pieces round a turning circle's rings: fine enough that the kerb and walk end
/// close to the street they open onto.
const RING_SEGMENTS: usize = 96;

/// A cul-de-sac's turning circle round `middle`, at the end of `street`: its asphalt and
/// its sidewalk, round from one of the street's flares to the other (its kerb is one of
/// [`kerb_runs`]), and the planted island in its middle, kerbed and guttered round.
fn cul_de_sac(
    group: &mut Node,
    street: Street,
    middle: DVec2,
    lawn: &Arc<crate::scene::Material>,
    flares: &[Flare],
) {
    let mut asphalt = RingGeometry {
        inner_radius: 0.0,
        outer_radius: CUL_DE_SAC_RADIUS,
        theta_segments: 28,
        ..RingGeometry::default()
    }
    .build();
    asphalt.rotate_x(-FRAC_PI_2);
    let mut node = flat(asphalt, ASPHALT);
    node.position = glam::DVec3::new(middle.x, STREET_TOP + CIRCLE_LIFT, middle.y);
    group.children.push(node);
    if let Some((start, end)) = ring_span(street, flares) {
        let (sweep, step) = (end - start, std::f64::consts::TAU / RING_SEGMENTS as f64);
        let walk = (
            CUL_DE_SAC_RADIUS + PARKWAY,
            CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK,
        );
        let mut surface = Surface::default();
        let pieces = (sweep / step).ceil().max(1.0) as usize;
        let at = |k: usize, radius: f64| {
            middle + DVec2::from_angle(start + sweep * k as f64 / pieces as f64) * radius
        };
        for k in 0..pieces {
            surface.polygon(
                &[
                    at(k, walk.0),
                    at(k + 1, walk.0),
                    at(k + 1, walk.1),
                    at(k, walk.1),
                ],
                WALK_TOP,
            );
        }
        group.children.push(flat(surface.mesh(), SIDEWALK_CONCRETE));
    }
    let island = CUL_DE_SAC_ISLAND + KERB_WIDTH;
    let nothing = |_: DVec2| false;
    group.children.push(flat(
        ring(
            middle,
            (island, island + ISLAND_GUTTER),
            STREET_TOP + GUTTER_LIFT,
            &nothing,
        ),
        GUTTER.1,
    ));
    group.children.push(flat(
        ring(middle, (CUL_DE_SAC_ISLAND, island), KERB_HEIGHT, &nothing),
        KERB,
    ));
    group
        .children
        .push(flat(ring_face(middle, island, 1.0, &nothing), KERB));
    let rim = |theta: f64| middle + DVec2::new(theta.cos(), theta.sin()) * CUL_DE_SAC_ISLAND;
    let ground = lawn_fan(middle, &rim, 0.1, ISLAND_GROUNDCOVER_TINT);
    group
        .children
        .push(shadow_receiver(Arc::new(ground), lawn.clone()));
}

/// A flat ring round `middle` from radius `inner` to `outer` at height `y`, in
/// [`RING_SEGMENTS`] pieces, leaving out those whose middle `skip` says.
fn ring(middle: DVec2, (inner, outer): (f64, f64), y: f64, skip: &dyn Fn(DVec2) -> bool) -> Mesh {
    let mut positions: Vec<f64> = Vec::new();
    for k in 0..RING_SEGMENTS {
        let (a, b) = (ring_point(k, 1.0), ring_point(k + 1, 1.0));
        if skip(middle + (a + b) * (inner + outer) / 4.0) {
            continue;
        }
        let corners = [
            middle + a * inner,
            middle + a * outer,
            middle + b * outer,
            middle + b * inner,
        ];
        let [p, q, r, _] = corners;
        // Up-facing when the corners turn clockwise seen from above (+z south).
        let turn = (q.y - p.y) * (r.x - p.x) - (q.x - p.x) * (r.y - p.y);
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

/// A kerb's face round `middle` at `radius`, from the asphalt's top to the kerb's,
/// facing out (`facing` +1) or in toward the middle (-1), in the pieces of [`ring`].
fn ring_face(middle: DVec2, radius: f64, facing: f64, skip: &dyn Fn(DVec2) -> bool) -> Mesh {
    let mut face = Wall::default();
    for k in 0..RING_SEGMENTS {
        let (a, b) = (ring_point(k, radius), ring_point(k + 1, radius));
        if skip(middle + (a + b) / 2.0) {
            continue;
        }
        let out = (a + b).normalize() * facing;
        face.quad(middle + a, middle + b, (STREET_TOP, KERB_HEIGHT), out);
    }
    face.mesh()
}

/// The `k`th of [`RING_SEGMENTS`] points round a circle of `radius` about the origin.
fn ring_point(k: usize, radius: f64) -> DVec2 {
    let angle = std::f64::consts::TAU * k as f64 / RING_SEGMENTS as f64;
    DVec2::new(angle.cos(), angle.sin()) * radius
}

/// A lot's front yard as the tract laid them out, in the lot's frame (`u` across, `v`
/// toward the street from the house's middle): the garage's driveway, the entry walk up
/// to the steps in the passage beside the office, and the office's front, from which the
/// planting bed and the lawn panel (or gravel) run out to the sidewalk.
struct FrontYard {
    /// Toward the door's half of the front (+1 the lot's `+across` side).
    side: f64,
    /// The driveway's middle and width.
    garage: f64,
    driveway: f64,
    /// The entry walk's middle.
    walk: f64,
    /// The house's front wall, the office's front and the back of the sidewalk (`v`).
    front: f64,
    office: f64,
    sidewalk: f64,
    /// Whether the yard is gravel with succulents rather than lawn, and whether its
    /// driveway has a strip of darker pavers along its outer side.
    gravel: bool,
    pavers: bool,
}

impl FrontYard {
    /// The driveway's edge on the door's side (`1.0`) or the outer side (`-1.0`).
    fn driveway_edge(&self, toward_door: f64) -> f64 {
        self.garage + self.side * toward_door * self.driveway / 2.0
    }

    /// The outer edge of the driveway's paving, its paver strip included.
    fn paving_edge(&self) -> f64 {
        let strip = if self.pavers { PAVER_STRIP.0 } else { 0.0 };
        self.driveway_edge(-1.0) - self.side * strip
    }

    /// The walk's edge toward the garage (`1.0`) or away from it (`-1.0`).
    fn walk_edge(&self, toward_garage: f64) -> f64 {
        self.walk - self.side * toward_garage * FRONT_WALK / 2.0
    }

    /// The office's bed: from its front to here (`v`).
    fn bed_front(&self) -> f64 {
        (self.office + OFFICE_BED).min(self.sidewalk - LEAST_FRONT_LAWN)
    }
}

/// Lot `k`'s front yard, placed from the openings its house's plan cuts; none for a house
/// without a garage front.
fn front_yard(lot: &Lot, k: usize) -> Option<FrontYard> {
    let (door, garage) = front_openings(lot)?;
    let design = lot.house.design();
    let side = (door - garage).signum();
    // The models' entry passage runs in along the corner away from the garage, as wide as
    // a share of the front.
    let passage = ENTRY_PASSAGE * lot.house.w / HOUSE_WIDTH;
    let front = lot.half_depth;
    let (_, _, depth) = porch(lot)?;
    Some(FrontYard {
        side,
        garage,
        driveway: design.wall_plan(design.door_wall).garage_width() + DRIVEWAY_PAST_DOOR,
        walk: side * (lot.house.w / 2.0 - passage / 2.0),
        front,
        office: front + depth - OFFICE_EAVE,
        sidewalk: front + lot.house.setback,
        gravel: k % 3 == 2,
        pavers: k.is_multiple_of(2),
    })
}

/// A lot's front yard's paving and ground: the driveway from the garage door across the
/// front yard, sidewalk and parkway to the kerb (on every other lot with a strip of
/// pavers along its outer side across the front yard), the entry walk from the sidewalk
/// to the steps, the bed of bark mulch along the office's front, a small lawn panel (on a
/// third of the lots gravel) between the driveway, the bed, the walk and the sidewalk, on
/// round the walk to the lot line, and outside the driveway to the other one.
fn front_yard_ground(
    group: &mut Node,
    lot: &Lot,
    k: usize,
    lawn: &Arc<crate::scene::Material>,
    runs: &[KerbRun],
) {
    let Some(yard) = front_yard(lot, k) else {
        return;
    };
    let span = |a: f64, b: f64| ((a + b) / 2.0, (a - b).abs());
    let apron = driveway_kerb(lot, &yard, runs);
    // Where it finds its kerb (lowered for it), the driveway runs on under the sidewalk's
    // middle and its apron from there to the kerb's back, following the kerb's curve.
    let kerb = if apron.is_some() {
        yard.sidewalk + SIDEWALK / 2.0
    } else {
        yard.sidewalk + SIDEWALK + PARKWAY
    };
    lot_slab(
        group,
        lot,
        (yard.garage, (yard.front + kerb) / 2.0),
        (yard.driveway, kerb - yard.front),
        0.055,
        DRIVEWAY,
    );
    if let Some(hits) = apron {
        let mut surface = Surface::default();
        let at = |k: usize| {
            let u = yard.garage + yard.driveway * (k as f64 / APRON_PIECES as f64 - 0.5);
            (lot.at(u, kerb), hits[k].2 + lot.front * APRON_UNDER_KERB)
        };
        // Along a street the kerb runs straight enough for one piece; round a circle or a
        // flare the apron follows its curve.
        let straight = hits
            .iter()
            .all(|hit| matches!(runs[hit.0].course, KerbCourse::Street { .. }));
        let pieces: Vec<usize> = if straight {
            vec![0, APRON_PIECES]
        } else {
            (0..=APRON_PIECES).collect()
        };
        for pair in pieces.windows(2) {
            let ((p0, q0), (p1, q1)) = (at(pair[0]), at(pair[1]));
            surface.polygon(&[p0, p1, q1, q0], 0.055);
        }
        group.children.push(flat(surface.mesh(), DRIVEWAY));
    }
    // Its control joints: one down the middle, two across, and the apron's at the walk.
    let joint = CONCRETE_JOINT_WIDTH;
    lot_slab(
        group,
        lot,
        (yard.garage, (yard.front + yard.sidewalk) / 2.0),
        (joint, yard.sidewalk - yard.front),
        0.055 + JOINT_LIFT,
        CONCRETE_JOINT,
    );
    for share in DRIVEWAY_JOINTS {
        let v = yard.front + (yard.sidewalk - yard.front) * share;
        lot_slab(
            group,
            lot,
            (yard.garage, v),
            (yard.driveway - 0.06, joint),
            0.055 + JOINT_LIFT,
            CONCRETE_JOINT,
        );
    }
    if yard.pavers {
        let (u, w) = span(yard.driveway_edge(-1.0), yard.paving_edge());
        lot_slab(
            group,
            lot,
            (u, (yard.front + yard.sidewalk) / 2.0),
            (w, yard.sidewalk - yard.front),
            0.056,
            PAVER_STRIP.1,
        );
    }
    let walk_from = yard.front + WALK_INTO_PASSAGE;
    lot_slab(
        group,
        lot,
        (yard.walk, (walk_from + yard.sidewalk) / 2.0),
        (FRONT_WALK, yard.sidewalk - walk_from),
        0.052,
        SIDEWALK_CONCRETE,
    );
    // The bed along the office's front, from the driveway to the walk.
    let (inner, inner_width) = span(yard.driveway_edge(1.0), yard.walk_edge(1.0));
    let bed = yard.bed_front();
    if inner_width > 0.05 && bed - yard.office > 0.05 {
        let mesh = ground_patch(
            lot,
            (inner, (yard.office + bed) / 2.0),
            (inner_width, bed - yard.office),
            0.045,
            (BED_CELL, BED_MULCH_TINT),
        );
        group
            .children
            .push(shadow_receiver(Arc::new(mesh), lawn.clone()));
    }
    // The panel before the bed, and round the walk's far side to the lot line.
    let line = if yard.side > 0.0 {
        lot.line_u(1)
    } else {
        lot.line_u(0)
    };
    let corner = yard.side * lot.house.w / 2.0;
    let other = if yard.side > 0.0 {
        lot.line_u(0)
    } else {
        lot.line_u(1)
    };
    let panels = [
        ((inner, inner_width), (bed, yard.sidewalk)),
        (
            span(yard.paving_edge(), other),
            (yard.front + 0.05, yard.sidewalk),
        ),
        (
            span(yard.walk_edge(-1.0), line),
            (yard.office, yard.sidewalk),
        ),
        (span(corner, line), (yard.front + 0.05, yard.office)),
    ];
    for ((u, w), (from, to)) in panels {
        let (to, from) = (to - 0.02, from + 0.01);
        if w < 0.1 || to - from < 0.1 {
            continue;
        }
        let middle = (u, (from + to) / 2.0);
        if yard.gravel {
            lot_slab(group, lot, middle, (w, to - from), 0.043, FRONT_GRAVEL);
        } else {
            let mesh = lawn_patch(lot, middle, (w, to - from), 0.042);
            group
                .children
                .push(shadow_receiver(Arc::new(mesh), lawn.clone()));
        }
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

/// The pieces of a median nose's half ring, its kerb's face following them.
const NOSE_SEGMENTS: usize = 12;

/// A round nose's half ring about `centre`, bulging toward `out`, from radius `inner`
/// (0 for its top) to `outer`, flat at height `y`.
fn nose(centre: DVec2, out: DVec2, (inner, outer): (f64, f64), y: f64) -> Mesh {
    let side = left(out);
    let point = |k: usize, radius: f64| {
        let angle = std::f64::consts::PI * k as f64 / NOSE_SEGMENTS as f64;
        centre + (side * angle.cos() + out * angle.sin()) * radius
    };
    let mut positions = Vec::new();
    for k in 0..NOSE_SEGMENTS {
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
/// returns, the patio paving the back yard from the house's back wall to the planting
/// strips along the back and side walls, and outside the back wall the bark mulch along
/// its foot.
fn yard_grounds(group: &mut Node, lots: &[Lot]) {
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
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let (inner0, inner1) = (u0 + 0.1, u1 - 0.1);
        if depth > 0.1 {
            lot_slab(
                group,
                lot,
                ((inner0 + inner1) / 2.0, back - depth / 2.0),
                (inner1 - inner0, depth),
                0.05,
                paving,
            );
        }
        // The planter strip: bark mulch at the ground, behind a low concrete header along
        // the patio, so it reads as a bed at the wall's foot rather than as the top of a
        // brown block with the wall's face below it.
        let planter = lot.rear_v() + 0.1 + YARD_PLANTER / 2.0;
        let (header, header_top) = BED_HEADER;
        let bed = ((u0 + u1) / 2.0, planter - header / 2.0);
        let size = (u1 - u0 - 0.2, YARD_PLANTER - header);
        group.children.push(shadow_receiver(
            Arc::new(ground_patch(
                lot,
                bed,
                size,
                0.042,
                (BED_CELL, BED_MULCH_TINT),
            )),
            lawn_material(),
        ));
        lot_slab(
            group,
            lot,
            ((u0 + u1) / 2.0, planter + YARD_PLANTER / 2.0 - header / 2.0),
            (u1 - u0 - 0.2, header),
            header_top,
            PATIO_CONCRETE,
        );
    }
}

/// The beds' bark mulch over the packed-dirt tile (linear), darker than the walls' block,
/// mottled over cells this long (m); and the low concrete header along a back yard's
/// planter strip, its width and its top (m).
const BED_MULCH_TINT: [f32; 3] = [0.17, 0.21, 0.46];
const BED_CELL: f64 = 1.5;
const BED_HEADER: (f64, f64) = (0.1, 0.075);

/// The front yards: the parkway's strip of clipped lawn between the kerb and the sidewalk
/// across each lot (its driveway apron and entry walk laid over it), and each lot's front
/// yard ([`front_yard_ground`]), their lawns in `lawn`.
fn front_lawns(
    group: &mut Node,
    lots: &[Lot],
    lawn: &Arc<crate::scene::Material>,
    kerbs: &[KerbRun],
) {
    for (k, lot) in lots.iter().enumerate() {
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let walk = lot.half_depth + lot.house.setback + SIDEWALK;
        let (near, far) = (walk + 0.02, walk + PARKWAY - KERB_WIDTH - 0.02);
        let mesh = lawn_patch(
            lot,
            ((u0 + u1) / 2.0, (near + far) / 2.0),
            (u1 - u0, far - near),
            0.045,
        );
        group
            .children
            .push(shadow_receiver(Arc::new(mesh), lawn.clone()));
        front_yard_ground(group, lot, k, lawn, kerbs);
    }
}

/// The ground's material, one for all of it (the ground, the planted ground, the mown
/// lawns, the planted islands and beds and the groundcover mats): the packed-dirt tile
/// under vertex tints.
fn lawn_material() -> Arc<crate::scene::Material> {
    static MATERIAL: crate::models::shared_values::SharedOnce<crate::scene::Material> =
        crate::models::shared_values::SharedOnce::new("senda_scenery::GROUND");
    MATERIAL
        .get_or_init(|| {
            let mut surface = ground_material(GroundKind::PackedDirt);
            surface.vertex_colors = true;
            surface.color = crate::scene::Color(0xffffff);
            Arc::new(surface)
        })
        .clone()
}

/// A lawn's tone at `at`: `tint`, its brightness swelling and falling gently over a few
/// metres and a little from cell to cell, as a real lawn's thicker and thinner patches.
fn lawn_tone(at: DVec2, tint: [f32; 3]) -> [f32; 3] {
    let (swell, grain) = LAWN_MOTTLE;
    let mottle = 1.0
        + swell * (at.x * 0.9 + at.y * 1.3).sin() * (at.y * 0.7 - at.x * 1.1).cos()
        + grain * (hash(at.x * 3.1 + at.y * 7.7) - 0.5);
    tint.map(|c| c * mottle as f32)
}

/// A round lawn: a fan about `centre` out to `rim(theta)`, with a ring halfway so its
/// mottle has somewhere to vary, at height `y` in `tint`.
fn lawn_fan(centre: DVec2, rim: &dyn Fn(f64) -> DVec2, y: f64, tint: [f32; 3]) -> Mesh {
    const SEGMENTS: usize = 24;
    let mut positions: Vec<f64> = vec![centre.x, y, centre.y];
    let mut colors = vec![lawn_tone(centre, tint)];
    for k in 0..SEGMENTS {
        let edge = rim(std::f64::consts::TAU * k as f64 / SEGMENTS as f64);
        for p in [centre.lerp(edge, 0.5), edge] {
            positions.extend([p.x, y, p.y]);
            colors.push(lawn_tone(p, tint));
        }
    }
    let point = |i: u32| {
        let i = i as usize * 3;
        DVec2::new(positions[i], positions[i + 2])
    };
    let mut indices: Vec<u32> = Vec::new();
    let mut triangle = |[a, b, c]: [u32; 3]| {
        // Up-facing when the corners turn clockwise seen from above (+z south).
        let (pa, pb, pc) = (point(a), point(b), point(c));
        let turn = (pb.y - pa.y) * (pc.x - pa.x) - (pb.x - pa.x) * (pc.y - pa.y);
        indices.extend(if turn > 0.0 { [a, b, c] } else { [a, c, b] });
    };
    for k in 0..SEGMENTS as u32 {
        let next = (k + 1) % SEGMENTS as u32;
        let (inner, outer) = (1 + 2 * k, 2 + 2 * k);
        let (next_inner, next_outer) = (1 + 2 * next, 2 + 2 * next);
        triangle([0, inner, next_inner]);
        triangle([inner, outer, next_outer]);
        triangle([inner, next_outer, next_inner]);
    }
    let count = positions.len() / 3;
    let normals: Vec<f64> = (0..count).flat_map(|_| [0.0, 1.0, 0.0]).collect();
    let mut mesh = Mesh::from_f64(&positions, &normals, &vec![0.0; count * 2], Some(indices));
    ground_uvs(&mut mesh, 0.0, 0.0);
    mesh.colors = colors;
    mesh
}

/// A lawn on `lot` round `(u, v)` of `(w, d)` (m) with its top `top` up, in
/// [`FRONT_LAWN_TINT`] mottled from cell to cell (a grid this fine, m) by [`lawn_tone`].
const LAWN_CELL: f64 = 0.6;

fn lawn_patch(lot: &Lot, middle: (f64, f64), size: (f64, f64), top: f64) -> Mesh {
    ground_patch(lot, middle, size, top, (LAWN_CELL, FRONT_LAWN_TINT))
}

/// A patch of ground on `lot` round `(u, v)` of `(w, d)` (m) with its top `top` up, in
/// `tint` over the ground's tile, mottled ([`lawn_tone`]) over a grid of `cell` (m).
fn ground_patch(
    lot: &Lot,
    (u, v): (f64, f64),
    (w, d): (f64, f64),
    top: f64,
    (cell, tint): (f64, [f32; 3]),
) -> Mesh {
    let (columns, rows) = (
        (w / cell).ceil().max(1.0) as usize,
        (d / cell).ceil().max(1.0) as usize,
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
            colors.push(lawn_tone(at, tint));
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

/// The bend park's ground: bark mulch between its lot lines, its kidney lawn (mown, as
/// the front lawns, in `lawn` with them, the mulch on the same tile) and the concrete walk
/// curving round it from the sidewalk and back.
fn bend_park_grounds(group: &mut Node, lines: &Lines, lawn: &Arc<crate::scene::Material>) {
    let Some(park) = bend_park(lines) else {
        return;
    };
    let mut mulch = quad(park.corners, 0.026);
    ground_uvs(&mut mulch, 0.0, 0.0);
    mulch.colors = mulch
        .positions
        .iter()
        .map(|p| {
            lawn_tone(
                DVec2::new(f64::from(p[0]), f64::from(p[2])),
                PARK_MULCH_TINT,
            )
        })
        .collect();
    group
        .children
        .push(shadow_receiver(Arc::new(mulch), lawn.clone()));
    let rim = |theta: f64| {
        let r = park.lawn(theta);
        park.at(theta.cos() * r, theta.sin() * r)
    };
    let grass = lawn_fan(park.middle, &rim, 0.036, FRONT_LAWN_TINT);
    group
        .children
        .push(shadow_receiver(Arc::new(grass), lawn.clone()));
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
    for theta in [
        1.5 * std::f64::consts::PI - 0.55,
        1.5 * std::f64::consts::PI + 0.55,
    ] {
        let r = park.lawn(theta) + PARK_WALK + 0.55;
        let at = park.at(theta.cos() * r, theta.sin() * r);
        bench(group, at, (park.middle - at).normalize());
    }
    let mailbox = park.at(park.half_width * 0.15, park.half_depth - 1.1);
    mailbox_bank(group, mailbox, park.out);
    let side = park.lawn(0.0) + PARK_WALK + 0.45;
    let (x, z) = park.at(-side, park.half_depth - 0.6).into();
    lamp(group, globe, (x, z), true);
}

/// The tract's park on the field behind Neblina's west row, laid out round the blue team's
/// spawn pads and clear of them: an oval walk round the open play lawn in the field's wide
/// south part, a walk from it north along the field between the pads and the planted
/// strip, out to a small round plaza between each pair of pads with two benches, a few
/// trees in ones and twos by the wall and the field's south edge, and low shrub beds at
/// its corners. Placed from the pads, every piece kept to the field by [`WestPark::fits`].
struct WestPark {
    /// The oval walk's middle and half extents (x, z).
    oval: (DVec2, DVec2),
    /// The walk from the oval north, and the spurs out to the plazas.
    walks: Vec<Vec<DVec2>>,
    plazas: Vec<DVec2>,
    trees: Vec<(DVec2, f64)>,
    beds: Vec<DVec2>,
}

/// The park's oval walk round its play lawn: its middle from the south-middle pad (m)
/// and its half extents (m); the walks' width; the plazas' radius; the distance (m) the
/// north walk keeps east of the pads' line; the trees' crowns (m) and how far (m) inside
/// the wall they stand; the beds' radius (m).
const PARK_OVAL: (DVec2, DVec2) = (DVec2::new(14.0, 10.5), DVec2::new(7.0, 4.5));
const WEST_PARK_WALK: f64 = 1.0;
/// The tops (m) of the park's oval walk, of the walks that branch off it (and the walks
/// out to the paseos) and of its plazas.
const PARK_OVAL_TOP: f64 = 0.04;
const PARK_BRANCH_TOP: f64 = 0.036;
const PARK_PLAZA_TOP: f64 = 0.044;
/// The walk from the park's walk to a paseo's, under both and over the paseo's granite.
const PASEO_LINK_TOP: f64 = 0.033;
const PARK_PLAZA: f64 = 1.8;
const PARK_WALK_EAST: f64 = 7.5;
const PARK_TREE_CROWN: (f64, f64) = (2.6, 3.4);
const PARK_TREE_HEIGHT: (f64, f64) = (3.2, 4.2);
const PARK_TREE_INSET: f64 = 2.6;
const PARK_BED: f64 = 1.4;

impl WestPark {
    /// The oval's middle line, closed.
    fn oval_line(&self) -> Vec<DVec2> {
        let (middle, half) = self.oval;
        (0..=48)
            .map(|k| {
                let angle = std::f64::consts::TAU * f64::from(k) / 48.0;
                middle + DVec2::new(angle.cos() * half.x, angle.sin() * half.y)
            })
            .collect()
    }

    /// Whether a piece reaching `room` (m) round `p` stays on the field: off the lots and
    /// their walls' mulch, inside the level's wall and clear of the pads and the pickups.
    fn fits(p: DVec2, room: f64, lines: &Lines, outer: &Outer) -> bool {
        Self::walk_fits(p, room, lines, outer)
            && outer
                .pickups
                .iter()
                .all(|pickup| pickup.distance(p) > OUTER_PICKUP_ROOM + room)
    }

    /// Whether a walk reaching `room` (m) round `p` stays on the field: as
    /// [`WestPark::fits`], but a walk runs on under the pickups, which stand on it.
    fn walk_fits(p: DVec2, room: f64, lines: &Lines, outer: &Outer) -> bool {
        lines.lot(p).0 < -(WALL_FOOT + room)
            && outer.inside(p) > room + 0.5
            && outer
                .pads
                .iter()
                .all(|pad| pad.distance(p) > OUTER_PAD_ROOM + 0.5 + room)
            && !is_senda_paved(p.x, p.y)
    }
}

/// The west park, from the blue team's pads down the field's west side; none without
/// four of them.
fn west_park() -> Option<WestPark> {
    let mut pads: Vec<DVec2> = spawn_positions(Team::Blue, 1.0)
        .into_iter()
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    pads.sort_by(|a, b| a.y.total_cmp(&b.y));
    if pads.len() < 4 {
        return None;
    }
    let n = pads.len();
    let middle = |a: DVec2, b: DVec2| (a + b) / 2.0;
    // Between the pads north of the southern pair.
    let plazas: Vec<DVec2> = pads[..n - 1]
        .windows(2)
        .skip(n.saturating_sub(4))
        .map(|pair| middle(pair[0], pair[1]))
        .collect();
    let south = pads[n - 2];
    let (offset, half) = PARK_OVAL;
    let centre = south + offset;
    let east = |z: f64| DVec2::new(south.x + PARK_WALK_EAST, z);
    // North from the oval's west end, past the pads, to the northern plaza.
    let mut north = vec![centre - DVec2::new(half.x, 0.0)];
    let northmost = plazas.first().map_or(f64::NEG_INFINITY, |plaza| plaza.y);
    north.extend(
        pads[..n - 1]
            .iter()
            .rev()
            .filter(|pad| pad.y > northmost)
            .map(|pad| east(pad.y)),
    );
    if let Some(&first) = plazas.first() {
        north.push(first + DVec2::new(PARK_PLAZA + 1.5, 1.5));
        north.push(first + DVec2::new(PARK_PLAZA - 0.2, 0.0));
    }
    let spurs = plazas
        .iter()
        .skip(1)
        .map(|&plaza| vec![east(plaza.y), plaza + DVec2::new(PARK_PLAZA - 0.2, 0.0)]);
    let walks: Vec<Vec<DVec2>> = std::iter::once(smooth(&north)).chain(spurs).collect();
    // Trees by the wall beside each plaza and two by the field's south edge; beds where
    // the walks meet and at the oval's ends.
    let wall = -ARENA + PARK_TREE_INSET;
    let mut trees: Vec<(DVec2, f64)> = plazas
        .iter()
        .flat_map(|plaza| {
            [
                (DVec2::new(wall, plaza.y - 2.4), plaza.y * 1.3),
                (DVec2::new(wall + 0.6, plaza.y + 2.2), plaza.y * 2.1),
            ]
        })
        .collect();
    trees.extend([
        (centre + DVec2::new(half.x + 3.5, half.y + 3.0), 4.1),
        (centre + DVec2::new(half.x + 6.0, half.y + 1.0), 5.7),
        (centre + DVec2::new(-half.x - 3.0, half.y + 3.5), 6.3),
    ]);
    let beds = vec![
        centre + DVec2::new(-half.x - 2.2, -half.y - 1.0),
        centre + DVec2::new(half.x + 2.0, -half.y - 0.5),
        centre + DVec2::new(-half.x + 1.0, half.y + 2.6),
    ];
    Some(WestPark {
        oval: (centre, half),
        walks,
        plazas,
        trees,
        beds,
    })
}

/// `points` with their corners rounded off (three rounds of corner cutting), ends kept.
fn smooth(points: &[DVec2]) -> Vec<DVec2> {
    let mut line = points.to_vec();
    for _ in 0..3 {
        if line.len() < 3 {
            break;
        }
        let mut next = vec![line[0]];
        for pair in line.windows(2) {
            next.push(pair[0].lerp(pair[1], 0.25));
            next.push(pair[0].lerp(pair[1], 0.75));
        }
        next.push(line[line.len() - 1]);
        line = next;
    }
    line
}

/// The west park's ground: its walks, plazas and the bark mulch of its beds, each left
/// out where it would leave the field.
fn west_park_grounds(group: &mut Node, lines: &Lines) {
    let Some(park) = west_park() else {
        return;
    };
    let outer = Outer::new(lines);
    let off = |p: DVec2| !WestPark::walk_fits(p, WEST_PARK_WALK / 2.0, lines, &outer);
    let half = WEST_PARK_WALK / 2.0;
    // The oval over the walks that leave it, which run on under it and under the
    // plazas (each a few millimetres apart, or they flicker where they overlap).
    group.children.push(flat(
        ribbon(&park.oval_line(), -half, half, PARK_OVAL_TOP, &off),
        SIDEWALK_CONCRETE,
    ));
    for line in &park.walks {
        group.children.push(flat(
            ribbon(line, -half, half, PARK_BRANCH_TOP, &off),
            SIDEWALK_CONCRETE,
        ));
    }
    let disc = |radius: f64, color: u32, y: f64, at: DVec2| {
        let mut geometry = RingGeometry {
            inner_radius: 0.0,
            outer_radius: radius,
            theta_segments: 16,
            ..RingGeometry::default()
        }
        .build();
        geometry.rotate_x(-FRAC_PI_2);
        let mut node = flat(geometry, color);
        node.position = glam::DVec3::new(at.x, y, at.y);
        node
    };
    for &plaza in &park.plazas {
        if WestPark::fits(plaza, PARK_PLAZA, lines, &outer) {
            group
                .children
                .push(disc(PARK_PLAZA, SIDEWALK_CONCRETE, PARK_PLAZA_TOP, plaza));
        }
    }
    for &bed in &park.beds {
        if WestPark::fits(bed, PARK_BED, lines, &outer) {
            group.children.push(disc(PARK_BED, BED_MULCH, 0.03, bed));
        }
    }
}

/// The west park's furniture and planting: two benches on each plaza and two on the
/// oval facing the play lawn, its trees and the low shrubs of its beds.
fn west_park_props(group: &mut Node, lines: &Lines) {
    let Some(park) = west_park() else {
        return;
    };
    let outer = Outer::new(lines);
    for &plaza in &park.plazas {
        if !WestPark::fits(plaza, PARK_PLAZA, lines, &outer) {
            continue;
        }
        for turn in [FRAC_PI_2, -FRAC_PI_2] {
            let facing = DVec2::new(turn.cos(), turn.sin());
            bench(group, plaza - facing * (PARK_PLAZA - 0.45), facing);
        }
    }
    let (middle, half) = park.oval;
    for side in [-1.0, 1.0] {
        let at = middle + DVec2::new(side * 2.2, -half.y - WEST_PARK_WALK / 2.0 - 0.45);
        if WestPark::fits(at, 0.8, lines, &outer) {
            bench(group, at, DVec2::new(0.0, 1.0));
        }
    }
    for &(at, seed) in &park.trees {
        let (least, most) = PARK_TREE_CROWN;
        let crown = least + (most - least) * hash(seed);
        if !WestPark::fits(at, crown / 2.0, lines, &outer) {
            continue;
        }
        let (low, high) = PARK_TREE_HEIGHT;
        let (x, z) = oak_site(at.x, at.y);
        let shape = TreeShape {
            x,
            z,
            w: crown,
            d: crown,
            h: low + (high - low) * hash(seed + 0.6),
        };
        put(group, tree_model(&shape, TreeDetail::Background), x, 0.0, z);
    }
    let bush = Arc::new(heart_geometry());
    for (k, &bed) in park.beds.iter().enumerate() {
        if !WestPark::fits(bed, PARK_BED, lines, &outer) {
            continue;
        }
        for n in 0..6 {
            let seed = (k * 11 + n) as f64 * 1.7;
            let angle = n as f64 / 6.0 * std::f64::consts::TAU + hash(seed);
            let reach = if n == 0 { 0.0 } else { PARK_BED * 0.6 };
            let at = bed + DVec2::new(angle.cos(), angle.sin()) * reach;
            let tone = PLANTED_LEAVES[(k + n) % PLANTED_LEAVES.len()];
            leafy(
                group,
                &bush,
                (at, 0.0),
                (0.32 + 0.1 * hash(seed + 0.3), 0.7),
                tone,
                seed,
            );
        }
    }
}

/// A park bench at `at` facing `facing`: a slatted seat and back on two metal legs.
fn bench(group: &mut Node, at: DVec2, facing: DVec2) {
    let yaw = facing.x.atan2(facing.y);
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
/// pergolas over the patios are covers, [`patio_cover`]): at most one small prop on a
/// patio, a round spa on a deep yard, a table under a patio umbrella or a potted shrub,
/// clear of the loggias; low shrubs along the planter strip at the back wall; the planted
/// beds of tall grass outside the rows' end lots; and the bend park's benches, mailboxes
/// and lamp.
fn yard_props() -> Node {
    let mut group = Node::group("senda-yards");
    let lines = Lines::new();
    let bush = Arc::new(heart_geometry());
    let canopy = Arc::new(cylinder_geometry(0.04, UMBRELLA.0, 0.2, 8));
    // The loggias and pergolas over the patios, each as its middle and half size in its
    // lot's frame: a prop keeps clear of them.
    let covers: Vec<(DVec2, DVec2)> = patio_covers(&senda_houses())
        .into_iter()
        .map(|cover| {
            (
                DVec2::new(cover.x, cover.z),
                DVec2::new(cover.w, cover.d) / 2.0,
            )
        })
        .collect();
    for (k, lot) in lines.lots.iter().enumerate() {
        let yaw = lot.front.x.atan2(lot.front.y);
        let part = |group: &mut Node, node: Node, (u, v): (f64, f64), y: f64| {
            let at = lot.at(u, v);
            put(group, rotated(node, 0.0, yaw, 0.0), at.x, y, at.y);
        };
        let (u0, u1) = (lot.line_u(0), lot.line_u(1));
        let planter = lot.rear_v() + 0.1 + YARD_PLANTER;
        let yard = back_yard(lot);
        let clear = |(u, v): (f64, f64), radius: f64| {
            u - radius > u0 + 0.15
                && u + radius < u1 - 0.15
                && v - radius > planter
                && v + radius < house_back(lot) - 0.1
                && covers.iter().all(|&(middle, half)| {
                    if lot.inside(middle) < 0.0 {
                        return true;
                    }
                    let (cu, cv) = lot.local(middle);
                    (u - cu).abs() > half.x + radius + 0.1 || (v - cv).abs() > half.y + radius + 0.1
                })
        };
        // At most one prop, in a back corner of the patio, the spa only on a deep yard.
        let side = turf_side(k);
        let corner = |radius: f64| {
            let edge = if side < 0.0 { u1 } else { u0 };
            (edge + side * (radius + 0.25), planter + radius + 0.1)
        };
        let mut taken: Option<((f64, f64), f64)> = None;
        let (spa, spa_height) = SPA;
        let (table, umbrella) = (TABLE.0, UMBRELLA.0);
        if k % 3 == 0 && yard >= SPA_YARD && clear(corner(spa), spa) {
            let at = corner(spa);
            part(
                &mut group,
                cylinder_part(spa, spa_height, SPA_COPING, 16),
                at,
                spa_height / 2.0,
            );
            part(
                &mut group,
                cylinder_part(spa - 0.1, 0.02, SPA_WATER, 16),
                at,
                spa_height + 0.002,
            );
            taken = Some((at, spa));
        } else if k % 3 == 1 && clear(corner(umbrella), umbrella) {
            // A small round table under a patio umbrella.
            let at = corner(umbrella);
            let (_, table_height, top) = TABLE;
            part(
                &mut group,
                cylinder_part(table, 0.03, top, 10),
                at,
                table_height,
            );
            let pole = UMBRELLA.1;
            part(
                &mut group,
                cylinder_part(0.02, pole, LAMP_METAL, 5),
                at,
                pole / 2.0,
            );
            let canvas = if k % 2 == 0 {
                UMBRELLA_CANVAS.0
            } else {
                UMBRELLA_CANVAS.1
            };
            part(
                &mut group,
                shadowed(canopy.clone(), paint(canvas)),
                at,
                pole - 0.05,
            );
            taken = Some((at, umbrella));
        } else if k % 3 == 2 && clear(corner(POT.0), POT.0) {
            let (radius, height, pot) = POT;
            let at = corner(radius);
            part(
                &mut group,
                cylinder_part(radius, height, pot, 10),
                at,
                height / 2.0,
            );
            let tone = YARD_SHRUB_LEAVES[k % YARD_SHRUB_LEAVES.len()];
            let spot = lot.at(at.0, at.1);
            leafy(
                &mut group,
                &bush,
                (spot, height - 0.05),
                (POT_PLANT, 0.9),
                tone,
                k as f64 * 2.9,
            );
            taken = Some((at, radius));
        }
        // Small, low leafy plants along the planter strip at the back wall, clear of the
        // prop and of its header.
        let mut u = u0 + 0.45;
        let mut n = 0usize;
        let bed = YARD_PLANTER - BED_HEADER.0;
        while u < u1 - 0.35 {
            let seed = (k * 41 + n) as f64 * 1.3;
            let (least, most) = YARD_SHRUB;
            let size = least + (most - least) * hash(seed + 0.4);
            let v = lot.rear_v() + 0.1 + bed * (0.45 + 0.1 * hash(seed));
            let free = taken.is_none_or(|((pu, pv), r)| (u - pu).hypot(v - pv) > r + size + 0.05);
            if free && hash(seed + 0.7) > YARD_SHRUB_GAPS {
                let count = YARD_BED_LEAVES.len();
                let tone = YARD_BED_LEAVES[(hash(seed + 1.9) * count as f64) as usize % count];
                leafy(
                    &mut group,
                    &bush,
                    (lot.at(u, v), 0.0),
                    (size, YARD_BED_SQUASH),
                    tone,
                    seed,
                );
            }
            u += YARD_SHRUB_STEP;
            n += 1;
        }
    }
    end_lot_beds(&mut group, &lines);
    let globe = Arc::new(sphere_geometry(1.0, 8, 6));
    bend_park_props(&mut group, &lines, &globe);
    west_park_props(&mut group, &lines);
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
            if side.neighbour.is_some() || side.walk.is_some() {
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

/// The land past the level's south and west walls, a belt of oaks over sage where a
/// tank cannot go.
struct Belt {
    boundary: Vec<DVec2>,
    /// The stretches of the level's wall that face south or west, each with the way out.
    walls: Vec<(Line, DVec2)>,
    way: Line,
}

impl Belt {
    fn new() -> Belt {
        let boundary = senda_boundary();
        let closed: Vec<DVec2> = boundary.iter().chain(boundary.first()).copied().collect();
        let walls = closed
            .windows(2)
            .filter_map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                let side = left((b - a).normalize_or_zero());
                let out = if inside_boundary(&boundary, (a + b) / 2.0 + side * 0.3) {
                    -side
                } else {
                    side
                };
                (out.x < -0.7 || out.y > 0.7).then(|| (Line(vec![a, b]), out))
            })
            .collect();
        Belt {
            boundary,
            walls,
            way: Street::SendaParkway.line(),
        }
    }

    /// How far (m) `p` stands outside the belt's walls: `None` inside the level or on
    /// Senda Parkway's verge.
    fn depth(&self, p: DVec2) -> Option<f64> {
        let way_reach = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + BELT_WAY_ROOM;
        if inside_boundary(&self.boundary, p) || self.way.distance(p) < way_reach {
            return None;
        }
        self.walls
            .iter()
            .map(|(wall, _)| wall.distance(p))
            .min_by(f64::total_cmp)
    }

    /// Where the belt's oaks stand, row by row out from the wall, the rows staggered.
    fn oaks(&self) -> Vec<DVec2> {
        let mut sites: Vec<DVec2> = Vec::new();
        for (row, &depth) in BELT_ROWS.iter().enumerate() {
            let outermost = row + 1 == BELT_ROWS.len();
            let (mut walked, mut k) = (0.0, 0usize);
            let mut next = BELT_SPACING * (0.5 + row as f64 / BELT_ROWS.len() as f64);
            for (wall, out) in &self.walls {
                let span = wall.length();
                while next < walked + span {
                    let seed = next * 0.731 + row as f64 * 5.3;
                    let wander = |salt: f64| (hash(seed + salt) - 0.5) * 2.0 * BELT_WANDER;
                    let (point, _) = wall.at(next - walked + wander(0.0));
                    let at = point + *out * (depth + wander(0.4));
                    next += BELT_SPACING;
                    k += 1;
                    if outermost && !k.is_multiple_of(BELT_OUTER_EVERY) {
                        continue;
                    }
                    // Clear of every wall, the ones round a corner too.
                    let clear = hill_patch(at) > BELT_GROVES[row]
                        && self.depth(at).is_some_and(|d| d > BELT_ROWS[0] * 0.6)
                        && sites.iter().all(|site| site.distance(at) > BELT_ROOM);
                    if clear {
                        sites.push(at);
                    }
                }
                walked += span;
            }
        }
        sites
    }

    /// The corners of the rectangle the belt lies in.
    fn bounds(&self) -> (DVec2, DVec2) {
        let reach = DVec2::splat(BELT_SAGE_REACH.1);
        let points = self
            .walls
            .iter()
            .flat_map(|(wall, _)| wall.0.iter().copied());
        let low = points.clone().fold(DVec2::MAX, DVec2::min);
        let high = points.fold(DVec2::MIN, DVec2::max);
        (low - reach, high + reach)
    }
}

/// Oaks and sycamores along Senda Parkway outside the walls, the band of oak and ficus
/// crowns down the slope behind Gaviota's south row past the level's wall, the
/// hillside's oaks in clusters past the arena's east edge, the belt of oaks past the
/// south and west walls, and their sage.
fn slope_trees(hills: &Hills) -> Node {
    let mut group = Node::group("senda-slope-trees");
    let mut place = |x: f64, z: f64, (w, h): (f64, f64)| {
        let (x, z) = oak_site(x, z);
        let shape = TreeShape { x, z, w, d: w, h };
        put(
            &mut group,
            tree_model(&shape, TreeDetail::Background),
            x,
            hills.height(x, z),
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
    // Past the south and west walls, a belt of oaks; the park's trees on the west field
    // are with its other furniture.
    let belt = Belt::new();
    for (k, at) in belt.oaks().into_iter().enumerate() {
        let seed = k as f64 * 1.37;
        let (least, most) = BELT_CROWN;
        let (low, high) = BELT_HEIGHT;
        place(
            at.x,
            at.y,
            (
                least + (most - least) * hash(seed),
                low + (high - low) * hash(seed + 0.6),
            ),
        );
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
    sage_clumps(&mut group, hills);
    belt_sage(&mut group, hills, &belt);
    batch_in_cells(&mut group, 40.0);
    // The fold oaks, far out round the ring, merged whole: a few draws however many
    // cells they span.
    let mut oaks = Node::group("");
    fold_oaks(&mut oaks, hills);
    batch(&mut oaks);
    group.children.push(oaks);
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
    form: LumpForm,
}

/// What a clump's lumps are drawn as.
#[derive(Clone, Copy, PartialEq)]
enum LumpForm {
    /// Leafy shrubs: sprig cards round a dark heart.
    Leafy,
    /// The heart alone, painted in the leaves' colour: far scenery, a fifth of the
    /// triangles.
    Painted,
    /// The grasses' tufts.
    Tuft,
}

const SAGE_CLUMPS: Clumps = Clumps {
    spacing: SAGE_SPACING,
    leaves: &SAGE_LEAVES,
    lump: SAGE_LUMP,
    squash: 0.75,
    form: LumpForm::Leafy,
};
const PLANTED_CLUMPS: Clumps = Clumps {
    spacing: PLANTED_SPACING,
    leaves: &PLANTED_LEAVES,
    lump: PLANTED_LUMP,
    squash: 0.6,
    form: LumpForm::Leafy,
};
const GRASS_CLUMPS: Clumps = Clumps {
    spacing: GRASS_SPACING,
    leaves: &GRASS_LEAVES,
    lump: GRASS_LUMP,
    squash: 1.1,
    form: LumpForm::Tuft,
};

/// Clumps of two or three lumps scattered off a grid over the rectangle from `low` to
/// `high` by a slow wobble, wherever `wanted` holds of a clump's middle, standing on
/// ground `base` metres up.
fn clumps(
    group: &mut Node,
    lump: &Arc<Mesh>,
    kind: &Clumps,
    (low, high): (DVec2, DVec2),
    base: &dyn Fn(DVec2) -> f64,
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
                    let spot = DVec2::new(cx + offset, cz - offset * 0.6);
                    match kind.form {
                        LumpForm::Leafy => {
                            let seed = cx * 3.1 + cz * 1.7 + n as f64;
                            leafy(
                                group,
                                lump,
                                (spot, base(spot)),
                                (size, kind.squash),
                                leaves,
                                seed,
                            );
                        }
                        LumpForm::Painted => {
                            // Darker than the leaves, as a leafy shrub reads beside its
                            // sunlit cards, and lower, a mound rather than a ball.
                            let mut heart =
                                shadowed(lump.clone(), paint(darker(leaves, PAINTED_SHADE)));
                            heart.scale =
                                glam::DVec3::new(size, size * kind.squash * 0.75, size) * 0.85;
                            heart.set_rotation_euler(0.0, hash(cx * 3.1 + cz * 1.7) * 3.0, 0.0);
                            let rise = size * kind.squash * 0.55;
                            put(group, heart, spot.x, base(spot) + rise, spot.y);
                        }
                        LumpForm::Tuft => {
                            let seed = cx * 2.3 + cz * 4.1 + n as f64;
                            tuft(group, spot, size, kind.squash, &[leaves], seed);
                        }
                    }
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
fn sage_clumps(group: &mut Node, hills: &Hills) {
    let lump = Arc::new(heart_geometry());
    let (near, far) = (ARENA + 3.0, ARENA + OUTER_SAGE_LEAFY);
    let (south, north) = (-ARENA - 20.0, ARENA + 20.0);
    let sparse = Clumps {
        spacing: OUTER_SAGE_SPACING,
        ..SAGE_CLUMPS
    };
    // Near the fence, where the play camera looks down on them, leafy shrubs; further
    // out, seen only from afar, their painted hearts.
    for (low, high, form) in [
        (near, far, LumpForm::Leafy),
        (far, ARENA + 40.0, LumpForm::Painted),
    ] {
        clumps(
            group,
            &lump,
            &Clumps { form, ..sparse },
            (DVec2::new(low, south), DVec2::new(high, north)),
            &|p| hills.height(p.x, p.y),
            &|p| hill_patch(p) > OUTER_SAGE_PATCH,
        );
    }
}

/// Sage under the belt's oaks past the south and west walls, in the hillside's patches.
fn belt_sage(group: &mut Node, hills: &Hills, belt: &Belt) {
    let lump = Arc::new(heart_geometry());
    let sage = Clumps {
        spacing: BELT_SAGE_SPACING,
        ..SAGE_CLUMPS
    };
    let (near, far) = BELT_SAGE_REACH;
    clumps(
        group,
        &lump,
        &sage,
        belt.bounds(),
        &|p| hills.height(p.x, p.y),
        &|p| {
            hill_patch(p) > BELT_SAGE_PATCH
                && belt
                    .depth(p)
                    .is_some_and(|depth| depth > near && depth < far)
        },
    );
}

/// The hills' oaks: a clump of rounded crowns on short trunks in each fold of the
/// foothills, olive-dark against the dry grass. Far scenery, so each oak is two leafy
/// masses over a trunk rather than a whole tree.
fn fold_oaks(group: &mut Node, hills: &Hills) {
    let heart = Arc::new(heart_geometry());
    for (at, seed) in hills.fold_oaks() {
        let ground = hills.height(at.x, at.y);
        let crown = FOLD_OAK_CROWN.0 + (FOLD_OAK_CROWN.1 - FOLD_OAK_CROWN.0) * hash(seed);
        let trunk = crown * 0.7;
        put(
            group,
            cylinder_part(0.16 * crown / 2.0, trunk + 0.6, OLIVE_BARK, 5),
            at.x,
            ground + (trunk - 0.6) / 2.0,
            at.y,
        );
        shrub_at(
            group,
            &heart,
            (at, ground + trunk * 0.55),
            (crown, 0.62),
            2,
            &FOLD_OAK_LEAVES,
            seed,
        );
    }
}

/// The ground's colour on the hills at `p` whose surface faces `normal`: dry tan grass
/// (gold where it faces the sun), olive scrub drifting over it, dark chaparral in
/// patches, down the gullies and on the steeper and north-facing slopes.
fn foothill_tint(p: DVec2, normal: [f32; 3]) -> [f32; 3] {
    let n = glam::DVec3::new(
        f64::from(normal[0]),
        f64::from(normal[1]),
        f64::from(normal[2]),
    );
    let steep = (1.0 - n.y).clamp(0.0, 1.0);
    // Facing north (-z) or toward the sun (south-west).
    let shaded = (-n.z).clamp(0.0, 1.0);
    let sunny = (n.dot(glam::DVec3::new(-0.76, 0.0, 0.65)) / (1.0 - n.y * n.y).max(1e-3).sqrt()
        * steep
        * 3.0)
        .clamp(0.0, 1.0);
    let wave = |a: f64, b: f64, c: f64| (p.x * a + p.y * b + c).sin();
    // Patches about 20 m across, and finer drifts.
    let patch =
        0.5 + 0.3 * wave(0.11, 0.07, 0.4) * wave(-0.05, 0.13, 1.9) + 0.2 * wave(0.29, -0.23, 2.7);
    let drift = 0.5 + 0.5 * wave(0.41, 0.33, 0.9) * wave(-0.27, 0.47, 3.1);
    let chaparral = rise((patch - 0.36) / 0.14 + steep * 1.5 + shaded * 1.4 - 0.2);
    let olive = rise((drift - 0.35) / 0.3) * 0.8;
    let mix = |a: f32, b: f32, t: f64| a + (b - a) * t as f32;
    std::array::from_fn(|i| {
        let grass = mix(FOOTHILL_TAN[i], FOOTHILL_GOLD[i], sunny * 0.8);
        let scrub = mix(grass, FOOTHILL_OLIVE[i], olive);
        mix(scrub, FOOTHILL_CHAPARRAL[i], chaparral)
    })
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
    let paseos = paseo_ways();
    let open = |p: DVec2| {
        if !off_paseos(&paseos, p, 0.3) {
            return false;
        }
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
    let (terrace, spurs) = bank_trails(&lines);
    let bank_walks: Vec<(Line, f64)> = spurs
        .into_iter()
        .map(|line| (Line(line), BANK_SPUR))
        .chain((terrace.len() > 1).then_some((Line(terrace), BANK_WALK)))
        .collect();
    contour_rows(
        &mut group,
        &bush,
        &lines.way,
        -1.0,
        (top + CREST_BASIN.0 + 0.9, ARENA),
        &CONTOUR_LEAVES,
        &|p| {
            let zone = zones(p, &lines);
            // On the carpet, a shrub here and there, clear of its walks.
            let on_carpet = hash(p.x * 5.3 + p.y * 2.9) < BANK_CARPET_SHRUBS
                && bank_walks
                    .iter()
                    .all(|(walk, width)| walk.distance(p) > width / 2.0 + CONTOUR_SIZE.1 + 0.2);
            zone.bank_land > 0.5
                && (zone.meadow < 0.5 || hash(p.x * 3.7 + p.y * 1.9) < MEADOW_SHRUBS)
                && (zone.bank_fall > BANK_COVER_DOWN + 0.04 || on_carpet)
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
        &|p| in_arena(p) && open(p) && zones(p, &lines).park < 0.2,
    );
    // The clipped shrubs along the downhill side of the bank's terrace walk.
    if let Some((walk, _)) = bank_walks.iter().find(|(_, width)| *width == BANK_WALK) {
        let mut s = TERRACE_SHRUB_STEP / 2.0;
        let mut n = 0usize;
        while s < walk.length() {
            let (point, along) = walk.at(s);
            s += TERRACE_SHRUB_STEP;
            n += 1;
            let out = left(along) * (BANK_WALK / 2.0 + TERRACE_SHRUB.1 + 0.15);
            // The side further down the bank.
            let at = [point + out, point - out]
                .into_iter()
                .max_by(|a, b| {
                    zones(*a, &lines)
                        .bank_fall
                        .total_cmp(&zones(*b, &lines).bank_fall)
                })
                .unwrap_or(point);
            let zone = zones(at, &lines);
            if zone.bank_land < 0.5 || zone.meadow > 0.5 || !in_arena(at) || !open(at) {
                continue;
            }
            let seed = n as f64 * 2.31;
            let (least, most) = TERRACE_SHRUB;
            let tone = CONTOUR_LEAVES[n % CONTOUR_LEAVES.len()];
            leafy(
                &mut group,
                &bush,
                (at, 0.0),
                (least + (most - least) * hash(seed), 0.8),
                tone,
                seed,
            );
        }
    }
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
    rounded_shrub(
        &mut group,
        (island, KERB_HEIGHT),
        (0.4, 0.45),
        ShrubForm::Mound,
        PLANTED_LEAVES[0],
        None,
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
    clumps(
        &mut group,
        &bush,
        &PLANTED_CLUMPS,
        everywhere,
        &|_| 0.0,
        &|p| {
            let zone = zones(p, &lines);
            let drift = (p.x * 0.29 - p.y * 0.13).sin() + (p.y * 0.23 + p.x * 0.05 + 0.4).cos();
            let near = trail(p);
            near > TRAIL_SHRUBS.0
                && near < TRAIL_SHRUBS.1
                && drift > PLANTED_DRIFT
                && zone.hillside >= 0.5
                && off_lots(p, 0.8)
                && open(p)
        },
    );
    // The hillside's coastal sage inside the arena, in its patches but for its trails.
    let hillside = (
        DVec2::new(HILLSIDE_FROM, -ARENA),
        DVec2::new(ARENA + 3.0, ARENA),
    );
    let sage = Clumps {
        spacing: HILL_SAGE_SPACING,
        ..SAGE_CLUMPS
    };
    // Clear of the view fence along the arena's east edge, inside it only, so the fence
    // and the mow strip at its foot read as the level's end.
    let fence = Line(
        lines
            .boundary
            .iter()
            .chain(lines.boundary.first())
            .copied()
            .collect(),
    );
    clumps(&mut group, &bush, &sage, hillside, &|_| 0.0, &|p| {
        zones(p, &lines).hillside >= 0.5
            && inside_boundary(&lines.boundary, p)
            && fence.distance(p) > FENCE_SAGE_ROOM
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
    clumps(&mut group, &lump, &GRASS_CLUMPS, near, &|_| 0.0, &|p| {
        (corner(p) || way_corner(p)) && off_lots(p, 0.3) && open(p) && !by_a_house(p, &houses)
    });
    // Down Senda Parkway's raised median and its south parkway.
    let trees = way_tree_sites();
    way_planting(&mut group, &lump, &lines, &trees);
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
        clumps(
            &mut group,
            &lump,
            &GRASS_CLUMPS,
            (low, high),
            &|_| 0.0,
            &|p| in_park_beds(&park, p) && open(p),
        );
    }
    front_yard_plants(&mut group, &lump, &lines.lots);
    // A ring of low shrubs round each turning circle's island tree, inside its kerb.
    for (k, middle) in Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .enumerate()
    {
        let reach = CUL_DE_SAC_ISLAND * ISLAND_RING;
        let count = (std::f64::consts::TAU * reach / ISLAND_SHRUB_STEP)
            .floor()
            .max(3.0) as usize;
        for n in 0..count {
            let seed = (k * 31 + n) as f64 * 1.3;
            let angle = std::f64::consts::TAU * (n as f64 + 0.3 * hash(seed)) / count as f64;
            let at = middle + DVec2::new(angle.cos(), angle.sin()) * reach;
            let size = ISLAND_SHRUB.0 + (ISLAND_SHRUB.1 - ISLAND_SHRUB.0) * hash(seed + 0.5);
            let tone = PLANTED_LEAVES[n % PLANTED_LEAVES.len()];
            rounded_shrub(
                &mut group,
                (at, 0.1),
                (size * 1.1, size * 1.2),
                ShrubForm::Mound,
                tone,
                None,
                seed,
            );
        }
    }
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

/// A clipped conical shrub (a dwarf juniper or boxwood cone) at `at`, about `radius`
/// across at its foot and `height` tall, tapering in tiers to its tip.
fn conical_shrub(group: &mut Node, at: DVec2, (radius, height): (f64, f64), color: u32, seed: f64) {
    rounded_shrub(
        group,
        (at, 0.0),
        (radius * FRONT_CONE_SPREAD, height),
        ShrubForm::Cone,
        color,
        None,
        seed,
    );
}

/// The front yards' planting: two or three small shrubs, clipped cones and rounded
/// mounds by turns, in the bed along each office's front; a cone by the garage's outer
/// corner; and on the gravel lots a few small agaves and a mounded shrub in the gravel.
fn front_yard_plants(group: &mut Node, lump: &Arc<Mesh>, lots: &[Lot]) {
    for (k, lot) in lots.iter().enumerate() {
        let Some(yard) = front_yard(lot, k) else {
            continue;
        };
        let (a, b) = (yard.driveway_edge(1.0), yard.walk_edge(1.0));
        let bed = (yard.office + yard.bed_front()) / 2.0;
        let count = 2 + k % 2;
        for n in 0..count {
            let t = (n as f64 + 0.5) / count as f64;
            let at = lot.at(a + (b - a) * t, bed);
            let seed = (k * 7 + n) as f64 * 1.9;
            let tone = FRONT_SHRUB_LEAVES[(k + n) % FRONT_SHRUB_LEAVES.len()];
            if (k + n).is_multiple_of(2) {
                let (low, high) = FRONT_CONE;
                let height = low + (high - low) * hash(seed);
                conical_shrub(group, at, (FRONT_CONE_RADIUS, height), tone, seed);
            } else {
                let size = FRONT_MOUND.0 + (FRONT_MOUND.1 - FRONT_MOUND.0) * hash(seed);
                // By some entries a mound in flower: daylilies and red salvia.
                let blossom = (hash(seed + 5.1) < FRONT_FLOWERING)
                    .then(|| FRONT_BLOSSOM[(k + n) % FRONT_BLOSSOM.len()]);
                rounded_shrub(
                    group,
                    (at, 0.0),
                    (size * 1.1, size * 1.5),
                    ShrubForm::Mound,
                    tone,
                    blossom,
                    seed,
                );
            }
        }
        // By the garage door's outer corner, where the lot leaves it room.
        let other = if yard.side > 0.0 {
            lot.line_u(0)
        } else {
            lot.line_u(1)
        };
        let corner = yard.paving_edge() - yard.side * (FRONT_CONE_RADIUS + 0.06);
        if (other - corner) * -yard.side > FRONT_CONE_RADIUS + 0.1 {
            let seed = k as f64 * 3.7;
            let at = lot.at(corner, yard.front + FRONT_CONE_RADIUS + 0.1);
            let size = (FRONT_CONE_RADIUS, FRONT_CONE.1);
            conical_shrub(group, at, size, FRONT_SHRUB_LEAVES[0], seed);
        }
        if yard.gravel {
            // On the gravel round the walk's far side, toward the lot line.
            let line = if yard.side > 0.0 {
                lot.line_u(1)
            } else {
                lot.line_u(0)
            };
            let (from, to) = (yard.walk_edge(-1.0), line);
            for n in 0..3 {
                let seed = (k * 13 + n) as f64 * 2.3;
                let u = from + (to - from) * (0.3 + 0.25 * n as f64);
                let v = yard.office + (yard.sidewalk - yard.office) * (0.25 + 0.5 * hash(seed));
                let size = FRONT_AGAVE.0 + (FRONT_AGAVE.1 - FRONT_AGAVE.0) * hash(seed + 0.4);
                agave(group, lump, lot.at(u, v), size, hash(seed + 0.9) * 6.0);
            }
            let at = lot.at(from + (to - from) * 0.55, yard.front + 0.35);
            let tone = FRONT_SHRUB_LEAVES[2];
            rounded_shrub(
                group,
                (at, 0.0),
                (FRONT_MOUND.1 * 1.2, FRONT_MOUND.1 * 1.3),
                ShrubForm::Mound,
                tone,
                None,
                k as f64,
            );
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
const FLOWERING_LEAVES: u32 = 0x55613e;
const FLOWERING_BLOSSOM: u32 = 0xb8382c;

/// Senda Parkway's planting, scenery a tank pushes through: down the raised median between
/// its trees and its openings, and down the south parkway between its kerb and its path,
/// broken round its trees and lamps and where Neblina meets it; tufts of straw-coloured
/// grass, blue-grey agaves and red-flowering shrubs over the groundcover, not a hedge.
fn way_planting(group: &mut Node, lump: &Arc<Mesh>, lines: &Lines, trees: &[DVec2]) {
    let half = Street::SendaParkway.half();
    let lamps: Vec<DVec2> = lamp_sites()
        .into_iter()
        .filter(|&(_, tall)| tall)
        .map(|(at, _)| at)
        .collect();
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
                let size = 0.3 + 0.08 * hash(seed + 3.3);
                rounded_shrub(
                    target,
                    (at, 0.0),
                    (size * 1.1, size * 1.3),
                    ShrubForm::Mound,
                    FLOWERING_LEAVES,
                    Some(FLOWERING_BLOSSOM),
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

/// An acorn street lamp at `(x, z)`: a row's, or one of Senda Parkway's tall ones.
fn lamp(group: &mut Node, globe_mesh: &Arc<Mesh>, (x, z): (f64, f64), tall: bool) {
    let (height, globe) = if tall {
        (WAY_LAMP_HEIGHT, WAY_LAMP_GLOBE)
    } else {
        (LAMP_HEIGHT, LAMP_GLOBE)
    };
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

/// A street lamp standing as cover, drawn at its footprint's middle.
pub fn street_lamp(group: &mut Node, tall: bool) {
    lamp(
        group,
        &Arc::new(sphere_geometry(1.0, 8, 6)),
        (0.0, 0.0),
        tall,
    );
}

/// The street lamps outside the level's wall (Senda Parkway's tall ones on its stretch
/// past the wall), as scenery; those inside are cover.
fn street_lamps() -> Node {
    let mut group = Node::group("senda-lamps");
    let globe = Arc::new(sphere_geometry(1.0, 8, 6));
    let boundary = senda_boundary();
    // Those inside the level are cover ([`street_lamp`]); the rest stand here.
    for (at, tall) in lamp_sites() {
        if !lamp_is_cover(&boundary, at) {
            lamp(&mut group, &globe, (at.x, at.y), tall);
        }
    }
    batch(&mut group);
    group
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
    wall_run(group, (w, d, h), color, turn, Coursing::Full);
}

/// A run of the level's own wall, built as a lot's ([`yard_wall`]) but with only every
/// other bed joint and no head joints: the long boundary is seen from further off, where
/// the finer joints fall under a pixel and only shimmer.
pub fn boundary_wall(group: &mut Node, (w, d, h): (f64, f64, f64), color: u32) {
    wall_run(group, (w, d, h), color, 0.0, Coursing::Sparse);
}

/// How much of the block's coursing a wall draws.
#[derive(Clone, Copy, PartialEq)]
enum Coursing {
    /// A bed joint under every course and the head joints in running bond.
    Full,
    /// A bed joint under every second course alone.
    Sparse,
}

fn wall_run(
    group: &mut Node,
    (w, d, h): (f64, f64, f64),
    color: u32,
    turn: f64,
    coursing: Coursing,
) {
    group.rotation = DQuat::from_rotation_y(turn);
    let (length, thick) = if w >= d { (w, d) } else { (d, w) };
    let mut run = Node::default();
    if w < d {
        run.rotation = DQuat::from_rotation_y(FRAC_PI_2);
    }
    match color {
        YARD_GATE_COLOR => gate_return(&mut run, length, h),
        YARD_VIEW_COLOR => view_fence_run(&mut run, length, thick, h, coursing),
        YARD_PERIMETER_COLOR => block_run(&mut run, length, thick, h, PERIMETER_BLOCK, coursing),
        _ => block_run(&mut run, length, thick, h, LOT_BLOCK, coursing),
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
/// its blocks in running bond, flat strips on both faces in one mesh; or, `Sparse`, a
/// bed joint under every second course alone.
fn coursing(run: &mut Node, length: f64, thick: f64, top: f64, look: BlockLook, detail: Coursing) {
    let (joint, proud, _) = BLOCK_JOINT;
    let paint_of = look.joint;
    let courses = if detail == Coursing::Sparse { 2.0 } else { 1.0 };
    let mut y = BLOCK_COURSE * courses;
    while y < top - 0.05 {
        along(
            run,
            box_part(length, joint, thick + 2.0 * proud, paint_of, 0.0),
            0.0,
            y,
        );
        y += BLOCK_COURSE * courses;
    }
    if detail == Coursing::Sparse {
        return;
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
fn block_run(run: &mut Node, length: f64, thick: f64, h: f64, look: BlockLook, detail: Coursing) {
    let cap = YARD_WALL_CAP.0;
    along(
        run,
        block_box(length, h - cap, thick, look),
        0.0,
        (h - cap) / 2.0,
    );
    wall_cap(run, length, thick, h, look);
    coursing(run, length, thick, h - cap, look, detail);
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
fn view_fence_run(run: &mut Node, length: f64, thick: f64, h: f64, detail: Coursing) {
    let cap = YARD_WALL_CAP.0;
    along(
        run,
        block_box(length, VIEW_BASE - cap, thick, LOT_BLOCK),
        0.0,
        (VIEW_BASE - cap) / 2.0,
    );
    wall_cap(run, length, thick, VIEW_BASE, LOT_BLOCK);
    coursing(run, length, thick, VIEW_BASE - cap, LOT_BLOCK, detail);
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
    let mut sites = Vec::new();
    let mut u = -length / 2.0 + side + spacing / 2.0;
    while u < length / 2.0 - spacing / 3.0 {
        if posts
            .iter()
            .all(|&post| (u - post).abs() > side / 2.0 + 0.02)
        {
            sites.push(u);
        }
        u += spacing;
    }
    if !sites.is_empty() {
        run.children.push(shadowed(
            Arc::new(pickets(&sites, picket, (VIEW_BASE, VIEW_BASE + tall))),
            paint(FENCE_STEEL),
        ));
    }
}

/// A run's pickets, `side` square at each of `sites` along x, from `y0` to `y1`: their
/// four sides alone in one mesh, since their ends sit on the base's cap and under a
/// pixel at the top.
fn pickets(sites: &[f64], side: f64, (y0, y1): (f64, f64)) -> Mesh {
    let half = side / 2.0;
    let mut positions: Vec<f64> = Vec::with_capacity(sites.len() * 16 * 3);
    let mut normals: Vec<f64> = Vec::with_capacity(sites.len() * 16 * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(sites.len() * 24);
    // Each face's two bottom corners (x, z) seen from outside it, left to right, and its
    // normal.
    let faces = [
        ([(-half, half), (half, half)], [0.0, 0.0, 1.0]),
        ([(half, half), (half, -half)], [1.0, 0.0, 0.0]),
        ([(half, -half), (-half, -half)], [0.0, 0.0, -1.0]),
        ([(-half, -half), (-half, half)], [-1.0, 0.0, 0.0]),
    ];
    for &u in sites {
        for ([(x0, z0), (x1, z1)], normal) in faces {
            let first = (positions.len() / 3) as u32;
            for (x, y, z) in [(x0, y0, z0), (x1, y0, z1), (x1, y1, z1), (x0, y1, z0)] {
                positions.extend([u + x, y, z]);
                normals.extend(normal);
            }
            indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
        }
    }
    let uvs = vec![0.0; positions.len() / 3 * 2];
    Mesh::from_f64(&positions, &normals, &uvs, Some(indices))
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

#[cfg(test)]
mod ground_tests {
    use super::*;

    /// Every vertex of `node`'s meshes in `material`, in world space.
    fn vertices_in(node: &Node, material: &Arc<crate::scene::Material>) -> Vec<glam::DVec3> {
        let mut found = Vec::new();
        node.traverse(glam::DMat4::IDENTITY, &mut |node, world| {
            let Some(drawable) = &node.drawable else {
                return;
            };
            if !Arc::ptr_eq(&drawable.material, material) {
                return;
            }
            for p in &drawable.mesh.positions {
                let at = glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
                found.push(world.transform_point3(at));
            }
        });
        found
    }

    /// The groundcover mats stand nowhere on a street's asphalt: the bank's carpet once
    /// spilled over Neblina where it leaves Senda Parkway.
    #[test]
    fn ground_mats_keep_off_the_streets() {
        let ground = lawn_material();
        for (name, node) in [
            ("bank", bank_planting()),
            ("outer slopes", outer_slopes()),
            ("greenbelt", greenbelt_scrub()),
        ] {
            let vertices = vertices_in(&node, &ground);
            assert!(!vertices.is_empty(), "{name}");
            for p in vertices.into_iter().filter(|p| p.y > 0.0) {
                let at = DVec2::new(p.x, p.z);
                for street in Street::ALL {
                    assert!(
                        street.line().distance(at) > street.half(),
                        "{name}'s mat stands on {street:?} at {at}"
                    );
                }
            }
        }
    }

    /// The planted ground lies over no paving: a cell reaches under a slab only from open
    /// ground beside it.
    #[test]
    fn planted_ground_keeps_off_the_paving() {
        let node = planted_ground(&Lines::new());
        let drawable = node.drawable.as_ref().expect("planted ground");
        let indices = drawable.mesh.indices.as_ref().expect("indexed");
        let paved = |k: u32| {
            let p = drawable.mesh.positions[k as usize];
            is_senda_paved(f64::from(p[0]), f64::from(p[2]))
        };
        // Each cell is two triangles over its four corners.
        for cell in indices.chunks(6) {
            assert!(
                !cell.iter().all(|&k| paved(k)),
                "a planted cell lies wholly on the paving"
            );
        }
    }
}
