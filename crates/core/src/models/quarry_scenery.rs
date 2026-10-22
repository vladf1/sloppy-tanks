//! Port of `quarry-scenery.ts`: Dusty Dig's retained scenery. The soil floor,
//! stepped quarry faces with talus and scree, the haul ramp, the stockpile and
//! sentinel butte, parked machinery, boundary dressing, spawn pads, and the
//! work-floor gravel. No per-frame animation, particles, lights or physics bodies.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::math::{js_round, smoothstep};
use crate::geometry::{Attribute, Mesh, VERTEX_ALPHA};
use crate::scene::{Material, Node};
use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
use crate::sim::data::ARENA;
use crate::sim::math::Random;
use crate::sim::types::{Team, Vec2};

use super::batching::batch;
use super::concrete_surfaces::concrete_wall;
use super::harbor_surfaces::steel_box;
use super::model_primitives::{
    TEAM_COLORS, box_part, cylinder_part, put, rotated, shadow_receiver,
};
use super::quarry_benches::{
    StockpileSpot, quarry_bench, quarry_butte, quarry_mesas, quarry_scree_spots,
    quarry_stockpile_geometry, quarry_stockpile_reach, quarry_stockpile_spot,
    quarry_talus_geometry, quarry_talus_point, quarry_talus_strips,
};
use super::quarry_machinery::{quarry_dump_truck, quarry_excavator};
use super::quarry_ramp::{
    QUARRY_RAMP, quarry_ramp_boulders, quarry_ramp_geometry, quarry_ramp_height, quarry_ramp_spoil,
};
use super::quarry_scree::quarry_scree;
use super::quarry_site_details::quarry_site_details;
use super::quarry_surfaces::{
    RockSetting, RubbleStone, STONE_SEAT, mesh_triangles, sand_drift_material, sandstone_rock,
    sandstone_rubble, sandstone_scatter, surface_height,
};
use super::quarry_terrain::{QUARRY_BANK_TOP, quarry_ground_drop, quarry_terrain, soil_uvs};

/// Where a loose stone at x/z rests: on `heap` (world triangles) or the drawn
/// ground, whichever is higher there.
fn rest_height(heap: &[[DVec3; 3]], x: f64, z: f64) -> f64 {
    let ground = 0.008 - quarry_ground_drop(x, z);
    surface_height(heap, x, z).map_or(ground, |y| y.max(ground))
}

/// Spoil the excavator has dug, heaped under its bucket on the north apron.
const DIG_HEAP: StockpileSpot = StockpileSpot {
    x: -13.0,
    z: -70.5,
    radius: 3.3,
    height: 1.9,
    seed: 7.0,
};

/// The machinery apron floor, where the lowest cuts and their talus stand.
const APRON: f64 = -1.8;
/// Talus stones per metre of wall toe.
const TALUS_STONES: f64 = 2.1;
/// Length of one run of talus stones along a wall, in metres: about one cell.
const TALUS_RUN: f64 = 40.0;
/// Loose rocks strewn beyond the boundary wall.
const BOUNDARY_ROCKS: u32 = 40;

/// `spoilSurface(geometry, fresh)`: map world x/z onto the soil bake and warm the
/// spoil toward the rock above. Fresh crushed stone is paler still.
fn spoil_surface(geometry: &mut Mesh, fresh: f64) {
    soil_uvs(geometry);
    geometry.colors = geometry
        .positions
        .iter()
        .map(|p| {
            let lift = smoothstep(f64::from(p[1]) - APRON, 0.2, 2.6);
            [
                (1.0 + 0.16 * lift + 0.2 * fresh) as f32,
                (1.0 + 0.14 * lift + 0.17 * fresh) as f32,
                (1.0 + 0.1 * lift + 0.12 * fresh) as f32,
            ]
        })
        .collect();
}

/// Shape of one spawn pad piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnPadShape {
    Disc,
    Dash,
    Chevron,
    Post,
    Cap,
}

/// One piece of a quarry spawn pad (`SpawnPadPiece`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPadPiece {
    /// Offset from the spawn point, in metres.
    pub dx: f64,
    pub dz: f64,
    /// Centre height of the piece.
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub color: u32,
    pub rot_y: f64,
    pub shape: SpawnPadShape,
}

/// `quarrySpawnPadPieces(team)`: a graded deployment pad (compacted gravel disc
/// with a worn centre, a ring of hazard dashes, chevrons pointing at the arena and
/// one team-capped beacon post). Everything stays ankle-high so pads read as
/// markings, never as cover.
pub fn quarry_spawn_pad_pieces(team: Team) -> Vec<SpawnPadPiece> {
    use SpawnPadShape::*;
    let piece = |shape, dx, dz, y, w, h, d, color, rot_y| SpawnPadPiece {
        dx,
        dz,
        y,
        w,
        h,
        d,
        color,
        rot_y,
        shape,
    };
    let mut pieces = vec![
        piece(Disc, 0.0, 0.0, 0.06, 2.6, 0.09, 2.6, 0x8f7c62, 0.0),
        piece(Disc, 0.0, 0.0, 0.115, 2.15, 0.03, 2.15, 0xcbb894, 0.0),
    ];
    for i in 0..12 {
        let angle = (f64::from(i) * PI) / 6.0;
        let color = if i % 2 == 1 { 0xc1aa64 } else { 0x383a35 };
        pieces.push(piece(
            Dash,
            angle.cos() * 2.38,
            angle.sin() * 2.38,
            0.115,
            0.45,
            0.03,
            0.45,
            color,
            0.0,
        ));
    }
    // Big wedge centred on the pad: the arms meet at an apex aiming at the arena
    // while the triangle's centroid sits exactly on the spawn point.
    let inward = if team == Team::Blue { 1.0 } else { -1.0 };
    let team_color = TEAM_COLORS[team.index()];
    for s in [-1.0, 1.0] {
        pieces.push(piece(
            Chevron,
            inward * 0.322,
            s * 0.527,
            0.13,
            2.2,
            0.03,
            0.26,
            team_color,
            s * 0.5 * inward,
        ));
    }
    let side = if team == Team::Blue { -1.0 } else { 1.0 };
    pieces.push(piece(
        Post,
        side * 2.5,
        0.0,
        0.45,
        0.06,
        0.9,
        0.06,
        0x535953,
        0.0,
    ));
    pieces.push(piece(
        Cap,
        side * 2.5,
        0.0,
        0.98,
        0.24,
        0.2,
        0.24,
        team_color,
        0.0,
    ));
    pieces
}

fn quarry_spawn_pad(group: &mut Node, team: Team, x: f64, z: f64) {
    for piece in quarry_spawn_pad_pieces(team) {
        let mesh = match piece.shape {
            SpawnPadShape::Disc => cylinder_part(piece.w, piece.h, piece.color, 20),
            SpawnPadShape::Post => cylinder_part(piece.w, piece.h, piece.color, 8),
            SpawnPadShape::Dash => steel_box(piece.w, piece.h, piece.d, piece.color),
            SpawnPadShape::Chevron | SpawnPadShape::Cap => {
                box_part(piece.w, piece.h, piece.d, piece.color, 0.0)
            }
        };
        put(
            group,
            rotated(mesh, 0.0, piece.rot_y, 0.0),
            x + piece.dx,
            piece.y,
            z + piece.dz,
        );
    }
}

/// Quarry faces per side: distance, height, base, depth.
const WEST_FACES: [[f64; 4]; 3] = [
    [77.0, 7.5, -1.8, 22.0],
    [95.0, 9.0, 2.5, 26.0],
    [115.0, 12.0, 7.8, 70.0],
];
const EAST_FACES: [[f64; 4]; 3] = [
    [78.0, 5.5, -1.8, 22.0],
    [93.0, 7.5, 2.5, 26.0],
    [114.0, 10.0, 7.8, 70.0],
];
/// Flank faces per side: distance, height, base.
const WEST_FLANKS: [[f64; 3]; 2] = [[80.0, 5.5, -1.8], [96.0, 8.5, 2.2]];
const EAST_FLANKS: [[f64; 3]; 2] = [[78.0, 6.5, -1.8], [97.0, 9.0, 2.2]];

/// Dusty Dig scenery (`QuarryScenery`, `dusty-dig-scenery`). Children: the floor,
/// the excavator, the haul truck (with its load), then the batched geology,
/// equipment and gravel groups.
pub fn quarry_scenery() -> Node {
    let mut root = Node::group("dusty-dig-scenery");
    let terrain = quarry_terrain();
    let soil = terrain
        .drawable
        .as_ref()
        .expect("floor mesh")
        .material
        .clone();
    root.children.push(terrain);
    let mut geology = Node::group("");
    let mut equipment = Node::group("");
    let mut rng = Random::new(9182.0);
    // Long, connected cuts. Offset benches expose broad shelves and a broken
    // skyline above the machinery apron. Each side digs to its own depth so the
    // excavation never reads as a rectangle.
    for side in [-1.0, 1.0] {
        let faces = if side < 0.0 { WEST_FACES } else { EAST_FACES };
        for [distance, height, base, depth] in faces {
            let mut face = quarry_bench(280.0, height, depth, distance + side * 17.0);
            if side < 0.0 {
                face.set_rotation_euler(0.0, PI, 0.0);
            }
            put(&mut geology, face, 0.0, base, side * distance);
        }
        let flanks = if side < 0.0 { WEST_FLANKS } else { EAST_FLANKS };
        for [distance, height, base] in flanks {
            let face = quarry_bench(155.0, height, 50.0, distance + side * 37.0);
            let face = rotated(face, 0.0, (side * PI) / 2.0, 0.0);
            put(&mut geology, face, side * distance, base, 0.0);
        }
    }
    // Talus heaps along every lowest wall toe, strewn with fragments that coarsen
    // downslope like sorted scree. The haul ramp shares the same spoil soil, and
    // every fragment lands in one merged rubble mesh. The spoil is a copy of the
    // soil so its batch stays apart from the scree mounds (batches group by Arc).
    let spoil = Arc::new((*soil).clone());
    let mut talus_rng = Random::new(2741.0);
    for strip in quarry_talus_strips() {
        let mut talus = quarry_talus_geometry(strip.x0, strip.x1, strip.seed);
        talus.rotate_y(strip.rot_y);
        talus.translate(strip.x, APRON, strip.z);
        let slope = mesh_triangles(&talus);
        spoil_surface(&mut talus, 0.0);
        geology
            .children
            .push(shadow_receiver(Arc::new(talus), spoil.clone()));
        // Stones in runs along the wall, each run landing in its own scenery cell.
        let (cos, sin) = (strip.rot_y.cos(), strip.rot_y.sin());
        let count = js_round((strip.x1 - strip.x0) * TALUS_STONES) as usize;
        let runs = ((strip.x1 - strip.x0) / TALUS_RUN).ceil() as usize;
        let mut stones: Vec<Vec<RubbleStone>> = vec![Vec::new(); runs];
        for _ in 0..count {
            let t = talus_rng.range(0.03, 0.95);
            let along = talus_rng.range(strip.x0, strip.x1);
            let p = quarry_talus_point(along, t, strip.seed);
            // Big blocks roll furthest; the fines stay up against the face.
            let boulder = t < 0.35 && talus_rng.next() < 0.1;
            let size = if boulder {
                talus_rng.range(0.65, 1.3)
            } else {
                talus_rng.range(0.15, 0.55) * (1.3 - t * 0.6)
            };
            let h = size * talus_rng.range(0.45, 0.8);
            let d = size * talus_rng.range(0.7, 1.2);
            let rot_y = talus_rng.range(-PI, PI);
            let shade = talus_rng.range(0.74, 1.02);
            let run = (((along - strip.x0) / TALUS_RUN) as usize).min(runs - 1);
            let (x, z) = (
                strip.x + p.x * cos + p.z * sin,
                strip.z - p.x * sin + p.z * cos,
            );
            stones[run].push(RubbleStone {
                x,
                y: rest_height(&slope, x, z) + h * STONE_SEAT,
                z,
                w: size,
                h,
                d,
                rot_y,
                shade,
            });
        }
        for run in stones.iter().filter(|run| !run.is_empty()) {
            geology.children.push(sandstone_scatter(run));
        }
    }
    // Crushed stone heaped under the conveyor head; coarse pieces roll to its toe
    // and rest on the heap's slope or the apron beyond it.
    let pile = quarry_stockpile_spot();
    let mut stockpile = quarry_stockpile_geometry(&pile);
    stockpile.translate(pile.x, APRON, pile.z);
    let heap = mesh_triangles(&stockpile);
    spoil_surface(&mut stockpile, 1.0);
    geology
        .children
        .push(shadow_receiver(Arc::new(stockpile), spoil.clone()));
    let pile_stones: Vec<RubbleStone> = (0..70)
        .map(|_| {
            let angle = talus_rng.range(0.0, PI * 2.0);
            let t = talus_rng.range(0.78, 1.02);
            let size = talus_rng.range(0.18, 0.5);
            let reach = quarry_stockpile_reach(&pile, angle) * t;
            let h = size * talus_rng.range(0.5, 0.8);
            let d = size * talus_rng.range(0.7, 1.2);
            let rot_y = talus_rng.range(-PI, PI);
            let shade = talus_rng.range(0.9, 1.15);
            let (x, z) = (pile.x + angle.cos() * reach, pile.z + angle.sin() * reach);
            let under = rest_height(&heap, x, z);
            RubbleStone {
                x,
                y: under + h * STONE_SEAT,
                z,
                w: size,
                h,
                d,
                rot_y,
                shade,
            }
        })
        .collect();
    geology.children.push(sandstone_scatter(&pile_stones));
    // The face the excavator is working: a heap of freshly dug spoil under its
    // bucket, with blasted blocks spilled round it (a stream of its own).
    let mut dig_rng = Random::new(5521.0);
    let mut dig = quarry_stockpile_geometry(&DIG_HEAP);
    dig.translate(DIG_HEAP.x, APRON, DIG_HEAP.z);
    let dug = mesh_triangles(&dig);
    spoil_surface(&mut dig, 0.6);
    geology
        .children
        .push(shadow_receiver(Arc::new(dig), spoil.clone()));
    let dig_stones: Vec<RubbleStone> = (0..40)
        .map(|i| {
            let angle = dig_rng.range(0.0, PI * 2.0);
            let t = dig_rng.range(0.3, 1.15);
            let size = if i < 8 {
                dig_rng.range(0.5, 0.95)
            } else {
                dig_rng.range(0.16, 0.45)
            };
            let reach = quarry_stockpile_reach(&DIG_HEAP, angle) * t;
            let h = size * dig_rng.range(0.5, 0.8);
            let (x, z) = (
                DIG_HEAP.x + angle.cos() * reach,
                DIG_HEAP.z + angle.sin() * reach,
            );
            let under = rest_height(&dug, x, z);
            RubbleStone {
                x,
                y: under + h * STONE_SEAT,
                z,
                w: size,
                h,
                d: size * dig_rng.range(0.7, 1.1),
                rot_y: dig_rng.range(-PI, PI),
                shade: dig_rng.range(0.85, 1.08),
            }
        })
        .collect();
    geology.children.push(sandstone_scatter(&dig_stones));
    // Local rubble stays outside the boundary; it never advertises nonexistent cover.
    for i in 0..BOUNDARY_ROCKS {
        let side = if i % 2 == 1 { -1.0 } else { 1.0 };
        let mut rock = sandstone_rock(
            0.7 + f64::from(i % 3) * 0.5,
            0.4 + f64::from(i % 4) * 0.25,
            1.2,
            i % 4,
            RockSetting::Ground,
        );
        rock.set_rotation_euler(0.0, rng.range(-1.0, 1.0), 0.0);
        let x = rng.range(-61.0, 61.0);
        let z = side * rng.range(64.0, 70.0);
        let y = 0.008 - quarry_ground_drop(x, z);
        put(&mut geology, rock, x, y, z);
    }
    flank_boulders(&mut geology);
    wall_drifts(&mut geology, &spoil);
    wall_spills(&mut geology);
    // Collapsed runouts interrupt the first terrace; the stacked sentinel gives
    // the north apron one recognizable landmark. All footprints stay outside
    // the playable boundary on the machinery apron.
    for spot in quarry_scree_spots() {
        geology.children.extend(quarry_scree(&spot, &soil));
    }
    // The haul ramp gives the parked machinery a believable way out of the pit.
    let ramp = quarry_ramp_geometry();
    let deck = mesh_triangles(&ramp);
    geology
        .children
        .push(shadow_receiver(Arc::new(ramp), spoil.clone()));
    for (i, boulder) in quarry_ramp_boulders().into_iter().enumerate() {
        let rock = sandstone_rock(
            boulder.size,
            boulder.size * 0.6,
            boulder.size * 0.85,
            i as u32 % 5,
            RockSetting::Ground,
        );
        let rock = rotated(rock, 0.0, boulder.rot_y, 0.0);
        let y = quarry_ramp_height(boulder.x, boulder.z) - 0.2;
        put(&mut geology, rock, boulder.x, y, boulder.z);
    }
    let spoil_chips: Vec<RubbleStone> = quarry_ramp_spoil()
        .into_iter()
        .map(|chip| RubbleStone {
            x: chip.x,
            y: rest_height(&deck, chip.x, chip.z) + chip.size * 0.45 * STONE_SEAT,
            z: chip.z,
            w: chip.size,
            h: chip.size * 0.45,
            d: chip.size * 0.8,
            rot_y: chip.rot_y,
            shade: 0.92,
        })
        .collect();
    geology.children.push(sandstone_scatter(&spoil_chips));
    geology.children.push(quarry_butte());
    geology.children.push(quarry_mesas());

    let excavator = rotated(quarry_excavator(), 0.0, -0.3, 0.0);
    put(&mut root, excavator, -24.0, -1.75, -68.0);
    // The south apron sits behind the gameplay camera, so the haul truck parks
    // on the east apron where the eastern spawn band sees it past the teeth.
    let mut truck = rotated(quarry_dump_truck(), 0.0, FRAC_PI_2 + 0.18, 0.0);
    truck.children.push(truck_load());
    put(&mut root, truck, 69.0, -1.75, 18.0);
    boundary_dressing(&mut equipment);
    for team in [Team::Blue, Team::Red] {
        for Vec2 { x, z } in spawn_positions(team, 1.0) {
            quarry_spawn_pad(&mut equipment, team, x, z);
        }
    }
    // Parked site office and stacked cut stone provide scale at the far quarry edge.
    put(
        &mut equipment,
        steel_box(11.0, 3.5, 5.0, 0x9eaca5),
        37.0,
        -0.04,
        -70.0,
    );
    put(
        &mut equipment,
        steel_box(11.6, 0.2, 5.6, 0x787e76),
        37.0,
        1.81,
        -70.0,
    );
    for x in [33.5, 36.5, 39.5] {
        put(
            &mut equipment,
            box_part(1.8, 1.25, 0.05, 0x526c72, 0.0),
            x,
            0.41,
            -67.47,
        );
    }
    for z in [-2.0, 2.0] {
        put(
            &mut geology,
            sandstone_rock(5.0, 2.2, 3.5, 0, RockSetting::Ground),
            -48.0,
            -1.79,
            68.0 + z,
        );
    }
    quarry_site_details(&mut equipment);
    // Geology stays unbatched: the renderer merges static scenery per material
    // and 60 m cell, so the long faces and drifts cull per cell instead of
    // drawing whole from anywhere.
    batch(&mut equipment);
    root.children.push(geology);
    root.children.push(equipment);
    root
}

/// The haul truck's bed (truck frame): floor height, x span and half width.
const TRUCK_BED: (f64, [f64; 2], f64) = (3.04, [-2.05, 4.95], 2.3);

/// Blasted sandstone heaped in the haul truck's bed: one merged rubble mesh of
/// angular blocks, dropped big ones first so smaller ones settle over them. Each
/// stone rests on the bed floor or on the stones already under it.
fn truck_load() -> Node {
    let mut rng = Random::new(6113.0);
    let (floor, [x0, x1], half) = TRUCK_BED;
    let (cx, length) = ((x0 + x1) / 2.0, (x1 - x0) / 2.0);
    let mut heap = LoadHeap::new(x0, x1, half, floor);
    let stones: Vec<RubbleStone> = (0..90)
        .map(|i| {
            let size = if i < 30 {
                rng.range(0.8, 1.3)
            } else {
                rng.range(0.35, 0.8)
            };
            let x = cx + rng.range(-1.0, 1.0) * (length - size * 0.4);
            let z = rng.range(-1.0, 1.0) * (half - size * 0.4);
            let h = size * rng.range(0.6, 0.9);
            let d = size * rng.range(0.7, 1.1);
            RubbleStone {
                x,
                y: heap.drop(x, z, size, h, d),
                z,
                w: size,
                h,
                d,
                rot_y: rng.range(-PI, PI),
                shade: rng.range(0.85, 1.12),
            }
        })
        .collect();
    sandstone_rubble(&stones)
}

/// Cell size of the truck load's height field, in metres.
const LOAD_CELL: f64 = 0.1;

/// The top of the truck load so far over a grid across the bed, so each dropped
/// stone lands on whatever is already under it.
struct LoadHeap {
    x0: f64,
    half: f64,
    columns: usize,
    rows: usize,
    tops: Vec<f64>,
}

impl LoadHeap {
    fn new(x0: f64, x1: f64, half: f64, floor: f64) -> Self {
        let columns = ((x1 - x0) / LOAD_CELL).ceil() as usize;
        let rows = (half * 2.0 / LOAD_CELL).ceil() as usize;
        Self {
            x0,
            half,
            columns,
            rows,
            tops: vec![floor; columns * rows],
        }
    }

    /// Cells within `radius` of x/z, with their distance.
    fn cells(&self, x: f64, z: f64, radius: f64) -> Vec<(usize, f64)> {
        let mut cells = Vec::new();
        for row in 0..self.rows {
            for column in 0..self.columns {
                let cx = self.x0 + (column as f64 + 0.5) * LOAD_CELL;
                let cz = -self.half + (row as f64 + 0.5) * LOAD_CELL;
                let distance = (cx - x).hypot(cz - z);
                if distance <= radius {
                    cells.push((row * self.columns + column, distance));
                }
            }
        }
        cells
    }

    /// Drop a stone of footprint `w` x `d` and height `h` at x/z: the centre
    /// height at which its underside rests on the highest point under its middle,
    /// after which its upper half becomes part of the heap. The heap takes the
    /// smallest a roughened stone can be ([`super::quarry_surfaces::roughen_stone`] shrinks a corner to
    /// 0.72), so stones dropped later sink into it a little rather than hover.
    fn drop(&mut self, x: f64, z: f64, w: f64, h: f64, d: f64) -> f64 {
        let under = self
            .cells(x, z, 0.25 * w.min(d))
            .into_iter()
            .map(|(cell, _)| self.tops[cell])
            .fold(f64::NEG_INFINITY, f64::max);
        let y = under + h * STONE_SEAT;
        let reach = 0.36 * w.min(d);
        for (cell, distance) in self.cells(x, z, reach) {
            let crown = y + 0.36 * h * (1.0 - distance / reach);
            self.tops[cell] = self.tops[cell].max(crown);
        }
        y
    }
}

/// Half-buried flank boulders break the east/west aprons. A separate stream keeps
/// the north/south rubble where it was; each candidate clears the parked truck,
/// the scree runouts and the playable boundary.
fn flank_boulders(geology: &mut Node) {
    let mut flank_rng = Random::new(1379.0);
    let r = &QUARRY_RAMP;
    let blocks = [
        [64.0, 76.0, 10.0, 26.0],   // haul truck bay
        [69.0, 76.5, -23.0, -1.0],  // east scree runout
        [-76.5, -69.0, -6.0, 18.0], // west scree runout
        [r.x0, r.x1, r.z0, r.z1],   // east haul ramp
    ];
    let mut placed = 0;
    let mut i = 0u32;
    while i < 40 && placed < 22 {
        let side = if i % 2 == 1 { -1.0 } else { 1.0 };
        let x = side * flank_rng.range(64.5, 71.0);
        let z = flank_rng.range(-50.0, 50.0);
        let variant = i % 5;
        i += 1;
        if blocks
            .iter()
            .any(|&[x0, x1, z0, z1]| x > x0 - 2.0 && x < x1 + 2.0 && z > z0 - 2.0 && z < z1 + 2.0)
        {
            continue;
        }
        let w = flank_rng.range(1.4, 2.8);
        let h = flank_rng.range(0.9, 1.8);
        let d = flank_rng.range(1.2, 2.4);
        let mut rock = sandstone_rock(w, h, d, variant, RockSetting::Ground);
        rock.set_rotation_euler(0.0, flank_rng.range(-PI, PI), 0.0);
        let y = 0.008 - quarry_ground_drop(x, z) - 0.14;
        put(geology, rock, x, y, z);
        placed += 1;
    }
}

/// Drift profile against the inner wall face: (metres from the face, height).
/// Low enough that tanks grinding along the wall only skim it.
const INNER_DRIFT: [(f64, f64); 5] = [
    (-0.1, 0.26),
    (0.25, 0.21),
    (0.6, 0.11),
    (0.95, 0.035),
    (1.3, 0.0),
];
/// Drift heaped on the bank outside the wall, over the falling ground.
const OUTER_DRIFT: [(f64, f64); 5] = [
    (-0.1, 0.5),
    (0.5, 0.38),
    (1.3, 0.18),
    (2.2, 0.05),
    (3.0, 0.0),
];
/// Length of one culled drift chunk along the wall, in metres.
const DRIFT_CHUNK: f64 = 30.0;

/// Windblown sand banked against both faces of the boundary wall, wavering in
/// height along it. Soil-textured, so it melts into the baked floor; receives
/// shadows only. Chunked so the renderer culls it per cell.
fn wall_drifts(geology: &mut Node, soil: &Arc<Material>) {
    let inner = ARENA;
    let outer = ARENA + BOUNDARY_THICKNESS;
    for side in [-1.0f64, 1.0] {
        for along_x in [false, true] {
            for (face, profile, outward) in
                [(inner, &INNER_DRIFT, -1.0), (outer, &OUTER_DRIFT, 1.0)]
            {
                let mut start = -outer;
                while start < outer - 0.01 {
                    let end = (start + DRIFT_CHUNK).min(outer);
                    let segments = ((end - start) / 0.9).ceil() as u32;
                    let mut positions = Vec::new();
                    let mut indices = Vec::new();
                    let columns = profile.len() as u32;
                    for i in 0..=segments {
                        let t = start + (end - start) * f64::from(i) / f64::from(segments);
                        let swell = 0.55
                            + 0.3 * (t * 0.21 + side * 1.7 + face).sin()
                            + 0.15 * (t * 0.77 + side).sin();
                        for &(distance, height) in profile {
                            let across = side * (face + outward * distance);
                            let (x, z) = if along_x { (t, across) } else { (across, t) };
                            let y = 0.008 - quarry_ground_drop(x, z) + height * swell;
                            positions.extend([x, y, z]);
                        }
                        if i < segments {
                            for c in 0..columns - 1 {
                                let a = i * columns + c;
                                let b = a + columns;
                                indices.extend([a, b, a + 1, b, b + 1, a + 1]);
                            }
                        }
                    }
                    geology
                        .children
                        .push(soil_strip(positions, indices, [1.05, 1.03, 1.0], soil));
                    start = end;
                }
            }
        }
    }
}

/// Sand fans spilled against the bastion wall's faces, where torn liners and
/// the wind have heaped it.
const WALL_SPILLS: u32 = 48;
/// Segments round one spill fan.
const SPILL_SEGMENTS: u32 = 8;

/// Soft fans of loose sand heaped against both faces of the bastion wall at
/// irregular intervals, feathering out over the floor. Soil-coloured through the
/// sand drift's blend; one merged mesh, one draw.
fn wall_spills(geology: &mut Node) {
    let mut rng = Random::new(6007.0);
    let mut positions: Vec<f64> = Vec::new();
    let mut alpha: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for k in 0..WALL_SPILLS {
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let along_x = k % 4 >= 2;
        let outer = k % 8 >= 6;
        let (face, outward) = if outer {
            (ARENA + BOUNDARY_THICKNESS + 0.03, 1.0)
        } else {
            (ARENA - 0.03, -1.0)
        };
        let t = rng.range(-57.0, 57.0);
        let radius = rng.range(1.0, 2.6);
        let height = rng.range(0.22, 0.45);
        let stretch = rng.range(0.8, 1.5);
        let point = |along: f64, across: f64| {
            let a = side * (face + outward * across);
            if along_x {
                (t + along, a)
            } else {
                (a, t + along)
            }
        };
        let ground = |x: f64, z: f64| 0.008 - quarry_ground_drop(x, z);
        let base = (positions.len() / 3) as u32;
        let (x, z) = point(0.0, 0.0);
        positions.extend([x, ground(x, z) + height, z]);
        alpha.push(0.95);
        for i in 0..=SPILL_SEGMENTS {
            let angle = PI * f64::from(i) / f64::from(SPILL_SEGMENTS);
            for (reach, rise, fade) in [(0.45, 0.45, 0.85), (1.0, 0.0, 0.0)] {
                let along = angle.cos() * radius * stretch * reach;
                let across = angle.sin() * radius * reach;
                let (x, z) = point(along, across);
                positions.extend([x, ground(x, z) + height * rise + 0.01, z]);
                alpha.push(fade);
            }
        }
        for i in 0..SPILL_SEGMENTS {
            let (a, b) = (base + 1 + i * 2, base + 3 + i * 2);
            indices.extend([base, a, b, a, a + 1, b, b, a + 1, b + 1]);
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &[], Some(indices));
    geometry.compute_vertex_normals();
    // Wind each fan to face up.
    let flipped: Vec<u32> = geometry
        .indices
        .as_ref()
        .map(|i| {
            i.chunks(3)
                .flat_map(|t| {
                    let p =
                        |v: u32| glam::DVec3::from(geometry.positions[v as usize].map(f64::from));
                    let up = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).y;
                    if up < 0.0 {
                        [t[0], t[2], t[1]]
                    } else {
                        [t[0], t[1], t[2]]
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    geometry.indices = Some(flipped);
    geometry.compute_vertex_normals();
    soil_uvs(&mut geometry);
    geometry.colors = vec![[1.04, 1.02, 0.98]; geometry.positions.len()];
    geometry.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
    geology
        .children
        .push(shadow_receiver(Arc::new(geometry), sand_drift_material()));
}

/// A soil-textured strip facing up, receiving shadows only.
fn soil_strip(
    positions: Vec<f64>,
    indices: Vec<u32>,
    tint: [f32; 3],
    soil: &Arc<Material>,
) -> Node {
    let mut geometry = Mesh::from_f64(&positions, &[], &[], Some(indices));
    // Face up whichever way the strip was laid out.
    geometry.compute_vertex_normals();
    if geometry.normals.iter().map(|n| n[1]).sum::<f32>() < 0.0 {
        let flipped: Vec<u32> = geometry
            .indices
            .as_ref()
            .map(|i| i.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect())
            .unwrap_or_default();
        geometry.indices = Some(flipped);
        geometry.compute_vertex_normals();
    }
    soil_uvs(&mut geometry);
    geometry.colors = vec![tint; geometry.positions.len()];
    shadow_receiver(Arc::new(geometry), soil.clone())
}

/// Survey stakes and buried footings along the boundary.
fn boundary_dressing(equipment: &mut Node) {
    for side in [-1.0f64, 1.0] {
        // Survey stakes frame the arena without fencing in
        // views. A few missing stakes and a slight lean keep the line from reading
        // as a fence.
        for step in 0..=14 {
            if (step + if side < 0.0 { 2 } else { 0 }) % 7 == 3 {
                continue;
            }
            let x = -56.0 + f64::from(step) * 8.0;
            let lean = 0.05 * (x * 2.3 + side).sin();
            let stake = rotated(box_part(0.13, 1.8, 0.13, 0xb6aea0, 0.0), 0.0, 0.0, lean);
            put(equipment, stake, x, 0.8, side * (QUARRY_BANK_TOP + 0.2));
            put(
                equipment,
                box_part(0.17, 0.32, 0.17, 0xa55e3f, 0.0),
                x - lean * 0.65,
                1.45,
                side * (QUARRY_BANK_TOP + 0.2),
            );
        }
        // A buried concrete footing closes the gap where the bank starts falling away
        // at the wall's outer face.
        let footing_length = QUARRY_BANK_TOP * 2.0 + 0.8;
        let footing = QUARRY_BANK_TOP - 0.3;
        put(
            equipment,
            concrete_wall(1.4, 0.55, footing_length),
            side * footing,
            -0.27,
            0.0,
        );
        put(
            equipment,
            concrete_wall(footing_length, 0.55, 1.4),
            0.0,
            -0.27,
            side * footing,
        );
    }
}
