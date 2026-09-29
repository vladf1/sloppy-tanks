//! Port of `quarry-scenery.ts`: Dusty Dig's retained scenery. The soil floor,
//! stepped quarry faces with talus and scree, the haul ramp, the stockpile and
//! sentinel butte, parked machinery, boundary dressing, spawn pads, and the
//! work-floor gravel. No per-frame animation, particles, lights or physics bodies.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::geometry::Mesh;
use crate::geometry::math::{js_round, smoothstep};
use crate::scene::{Material, Node};
use crate::sim::arena::spawn_positions;
use crate::sim::math::Random;
use crate::sim::types::{Team, Vec2};

use super::batching::batch;
use super::concrete_surfaces::concrete_wall;
use super::harbor_surfaces::steel_box;
use super::model_primitives::{TEAM_COLORS, box_part, cylinder_part, put, rotated};
use super::quarry_benches::{
    quarry_bench, quarry_butte, quarry_butte_spot, quarry_scree_spots, quarry_stockpile_geometry,
    quarry_stockpile_reach, quarry_stockpile_spot, quarry_talus_geometry, quarry_talus_point,
    quarry_talus_strips,
};
use super::quarry_machinery::{quarry_dump_truck, quarry_excavator};
use super::quarry_ramp::{
    QUARRY_RAMP, quarry_ramp_boulders, quarry_ramp_geometry, quarry_ramp_height, quarry_ramp_spoil,
};
use super::quarry_scree::quarry_scree;
use super::quarry_site_details::quarry_site_details;
use super::quarry_soil::QUARRY_TERRAIN_EXTENT;
use super::quarry_surfaces::{RubbleStone, sandstone_rock, sandstone_rubble};
use super::quarry_terrain::quarry_terrain;

/// The machinery apron floor, where the lowest cuts and their talus stand.
const APRON: f64 = -1.8;

/// `spoilSurface(geometry, fresh)`: map world x/z onto the soil bake and warm the
/// spoil toward the rock above. Fresh crushed stone is paler still.
fn spoil_surface(geometry: &mut Mesh, fresh: f64) {
    geometry.uvs = geometry
        .positions
        .iter()
        .map(|p| {
            [
                (f64::from(p[0]) / QUARRY_TERRAIN_EXTENT + 0.5) as f32,
                (0.5 - f64::from(p[2]) / QUARRY_TERRAIN_EXTENT) as f32,
            ]
        })
        .collect();
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

/// Dusty Dig scenery (`QuarryScenery`, `dusty-dig-scenery`).
pub struct QuarryScenery {
    /// Children: the floor, the excavator, the haul truck (with its load), then the
    /// batched geology, equipment and gravel groups.
    pub root: Node,
}

impl Default for QuarryScenery {
    fn default() -> Self {
        Self::new()
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

impl QuarryScenery {
    pub fn new() -> Self {
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
        // Long, connected cuts replace the repeated perimeter boulders. Offset benches
        // expose broad shelves and a broken skyline above the machinery apron. Each
        // side digs to its own depth so the excavation never reads as a rectangle.
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
        // every fragment lands in one merged rubble mesh.
        let spoil = Arc::new(Material {
            vertex_colors: true,
            ..(*soil).clone()
        });
        let mut talus_rng = Random::new(2741.0);
        let mut talus_stones = Vec::new();
        for strip in quarry_talus_strips() {
            let mut talus = quarry_talus_geometry(strip.x0, strip.x1, strip.seed);
            talus.rotate_y(strip.rot_y);
            talus.translate(strip.x, APRON, strip.z);
            spoil_surface(&mut talus, 0.0);
            geology
                .children
                .push(Node::mesh(Arc::new(talus), spoil.clone()));
            let (cos, sin) = (strip.rot_y.cos(), strip.rot_y.sin());
            let count = js_round((strip.x1 - strip.x0) * 3.4) as usize;
            for _ in 0..count {
                let t = talus_rng.range(0.03, 0.95);
                let along = talus_rng.range(strip.x0, strip.x1);
                let p = quarry_talus_point(along, t, strip.seed);
                let boulder = t < 0.35 && talus_rng.next() < 0.08;
                let size = if boulder {
                    talus_rng.range(0.65, 1.3)
                } else {
                    talus_rng.range(0.15, 0.55) * (1.3 - t * 0.6)
                };
                let h = size * talus_rng.range(0.45, 0.8);
                let d = size * talus_rng.range(0.7, 1.2);
                let rot_y = talus_rng.range(-PI, PI);
                let shade = talus_rng.range(0.74, 1.02);
                talus_stones.push(RubbleStone {
                    x: strip.x + p.x * cos + p.z * sin,
                    y: APRON + p.y + size * 0.1,
                    z: strip.z - p.x * sin + p.z * cos,
                    w: size,
                    h,
                    d,
                    rot_y,
                    shade,
                });
            }
        }
        // Crushed stone heaped under the conveyor head; coarse pieces roll to its toe.
        let pile = quarry_stockpile_spot();
        let mut stockpile = quarry_stockpile_geometry(&pile);
        stockpile.translate(pile.x, APRON, pile.z);
        spoil_surface(&mut stockpile, 1.0);
        geology
            .children
            .push(Node::mesh(Arc::new(stockpile), spoil.clone()));
        for _ in 0..70 {
            let angle = talus_rng.range(0.0, PI * 2.0);
            let t = talus_rng.range(0.78, 1.02);
            let size = talus_rng.range(0.18, 0.5);
            let reach = quarry_stockpile_reach(&pile, angle) * t;
            let h = size * talus_rng.range(0.5, 0.8);
            let d = size * talus_rng.range(0.7, 1.2);
            let rot_y = talus_rng.range(-PI, PI);
            let shade = talus_rng.range(0.9, 1.15);
            talus_stones.push(RubbleStone {
                x: pile.x + angle.cos() * reach,
                y: APRON + 0.0f64.max(pile.height * (1.0 - t.powf(1.08))) + size * 0.1,
                z: pile.z + angle.sin() * reach,
                w: size,
                h,
                d,
                rot_y,
                shade,
            });
        }
        geology.children.push(sandstone_rubble(&talus_stones));
        // Local rubble stays outside the boundary; it never advertises nonexistent cover.
        for i in 0..65u32 {
            let side = if i % 2 == 1 { -1.0 } else { 1.0 };
            let mut rock = sandstone_rock(
                0.7 + f64::from(i % 3) * 0.5,
                0.4 + f64::from(i % 4) * 0.25,
                1.2,
                i % 4,
            );
            rock.set_rotation_euler(0.0, rng.range(-1.0, 1.0), 0.0);
            let x = rng.range(-61.0, 61.0);
            let z = side * rng.range(64.0, 70.0);
            let y = 0.008 - 1.8f64.min((z.abs() - 60.0) * 0.3);
            put(&mut geology, rock, x, y, z);
        }
        flank_boulders(&mut geology);
        // Collapsed runouts interrupt the first terrace; the stacked sentinel gives
        // the north apron one recognizable landmark. All footprints stay outside
        // the playable boundary on the machinery apron.
        for spot in quarry_scree_spots() {
            geology.children.extend(quarry_scree(&spot, &soil));
        }
        // The haul ramp gives the parked machinery a believable way out of the pit.
        geology
            .children
            .push(Node::mesh(Arc::new(quarry_ramp_geometry()), spoil.clone()));
        for (i, boulder) in quarry_ramp_boulders().into_iter().enumerate() {
            let rock = sandstone_rock(
                boulder.size,
                boulder.size * 0.6,
                boulder.size * 0.85,
                i as u32 % 5,
            );
            let rock = rotated(rock, 0.0, boulder.rot_y, 0.0);
            let y = quarry_ramp_height(boulder.x, boulder.z) - 0.2;
            put(&mut geology, rock, boulder.x, y, boulder.z);
        }
        for (i, chip) in quarry_ramp_spoil().into_iter().enumerate() {
            let rock = sandstone_rock(chip.size, chip.size * 0.45, chip.size * 0.8, i as u32 % 7);
            let rock = rotated(rock, 0.0, chip.rot_y, 0.0);
            let y = quarry_ramp_height(chip.x, chip.z) - 0.08;
            put(&mut geology, rock, chip.x, y, chip.z);
        }
        let butte_spot = quarry_butte_spot();
        let butte = quarry_butte(butte_spot.scale, butte_spot.rot_y);
        put(
            &mut geology,
            butte,
            butte_spot.x,
            butte_spot.base_y,
            butte_spot.z,
        );

        let excavator = rotated(quarry_excavator(), 0.0, -0.3, 0.0);
        put(&mut root, excavator, -24.0, -1.75, -68.0);
        // The south apron sits behind the gameplay camera, so the haul truck parks
        // on the east apron where the eastern spawn band sees it past the teeth.
        let mut truck = rotated(quarry_dump_truck(), 0.0, FRAC_PI_2 + 0.18, 0.0);
        // Load the truck with a few large chunks instead of dozens of individual stones.
        for i in 0..5u32 {
            put(
                &mut truck,
                sandstone_rock(2.6, 1.25, 2.2, i % 4),
                -0.6 + f64::from(i % 3) * 1.8,
                3.8,
                if i % 2 == 1 { -1.0 } else { 1.0 },
            );
        }
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
                sandstone_rock(5.0, 2.2, 3.5, 0),
                -48.0,
                -1.79,
                68.0 + z,
            );
        }
        // Gravel a few centimetres high: shadows would only cost a pass, never read.
        let mut gravel = Node::group("");
        quarry_site_details(&mut equipment, &mut gravel);
        batch(&mut geology);
        batch(&mut equipment);
        batch(&mut gravel);
        for mesh in &mut gravel.children {
            if let Some(drawable) = &mut mesh.drawable {
                drawable.cast_shadow = false;
            }
        }
        root.children.push(geology);
        root.children.push(equipment);
        root.children.push(gravel);
        Self { root }
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
        let mut rock = sandstone_rock(w, h, d, variant);
        rock.set_rotation_euler(0.0, flank_rng.range(-PI, PI), 0.0);
        let y = 0.008 - 1.8f64.min((x.abs() - 60.0) * 0.3) - 0.14;
        put(geology, rock, x, y, z);
        placed += 1;
    }
}

/// Survey stakes, hazard paint, buried footings and wall joints along the boundary.
fn boundary_dressing(equipment: &mut Node) {
    for side in [-1.0f64, 1.0] {
        // Survey stakes and boundary hazard paint frame the arena without fencing in
        // views. A few missing stakes and a slight lean keep the line from reading
        // as a fence.
        for step in 0..=14 {
            let x = -56.0 + f64::from(step) * 8.0;
            let index = (x + 56.0) / 8.0 + if side < 0.0 { 2.0 } else { 0.0 };
            if index % 7.0 == 3.0 {
                continue;
            }
            let lean = 0.05 * (x * 2.3 + side).sin();
            let stake = rotated(box_part(0.13, 1.8, 0.13, 0xb6aea0, 0.0), 0.0, 0.0, lean);
            put(equipment, stake, x, 0.8, side * 61.2);
            put(
                equipment,
                box_part(0.17, 0.32, 0.17, 0xa55e3f, 0.0),
                x - lean * 0.65,
                1.45,
                side * 61.2,
            );
        }
        // Short yellow/black hazard bands: a lone dark panel on the shaded face read
        // as a slot through the wall.
        for step in 0..=11 {
            let z = -55.0 + f64::from(step) * 10.0;
            for k in -2i32..=2 {
                let color = if k % 2 != 0 { 0x3f3f3a } else { 0xd0b35a };
                put(
                    equipment,
                    box_part(0.04, 0.26, 0.36, color, 0.0),
                    side * 59.98,
                    0.74,
                    z + f64::from(k) * 0.36,
                );
            }
        }
        // A buried concrete footing closes the gap where the apron starts falling
        // away under the wall's outer half.
        put(
            equipment,
            concrete_wall(1.4, 0.55, 122.8),
            side * 60.7,
            -0.27,
            0.0,
        );
        put(
            equipment,
            concrete_wall(122.8, 0.55, 1.4),
            0.0,
            -0.27,
            side * 60.7,
        );
        // Precast segment joints score both faces of the boundary wall every four metres.
        for step in 0..=29 {
            let t = -58.0 + f64::from(step) * 4.0;
            put(
                equipment,
                box_part(0.05, 1.16, 1.02, 0x8c877d, 0.0),
                side * 60.5,
                0.58,
                t,
            );
            put(
                equipment,
                box_part(1.02, 1.16, 0.05, 0x8c877d, 0.0),
                t,
                0.58,
                side * 60.5,
            );
        }
    }
}
