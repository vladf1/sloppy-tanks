//! Loose stones rest on what is under them: every small, separate piece of the quarry's
//! sandstone (scree, chips, rubble, the haul truck's load) and Pine Village's creek-bank
//! stones touches a surface below it, or is buried, never hanging in the air.

use std::collections::HashMap;

use glam::{DMat4, DVec3};
use sloppy_core::models::effects_scenery::SANDSTONE;
use sloppy_core::models::{CoverShape, MapTheme, TreeDetail, build_scenery, cover_model};
use sloppy_core::scene::{Effect, Node};
use sloppy_core::sim::Simulation;
use sloppy_core::sim::map_options::MapId;

/// Pieces wider or taller than this, in metres, are landforms, not loose stones.
const LOOSE: f64 = 3.0;
/// How far a stone's underside may stand clear of the surface under it, in metres:
/// less than a faceted stone tilted on a steep heap shows.
const CLEARANCE: f64 = 0.04;
/// Edge of the grid cells that index triangles by x/z, in metres.
const CELL: f64 = 2.0;

/// One connected piece of a mesh (triangles sharing corners), in world space.
struct Piece {
    loose_stone: bool,
    triangles: usize,
    lo: DVec3,
    hi: DVec3,
    /// Corners in the lowest eighth of the piece.
    underside: Vec<DVec3>,
}

#[derive(Default)]
struct World {
    triangles: Vec<([DVec3; 3], usize)>,
    cells: HashMap<(i32, i32), Vec<usize>>,
    pieces: Vec<Piece>,
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn cell(x: f64, z: f64) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

impl World {
    fn add(
        &mut self,
        node: &Node,
        parent: DMat4,
        stones: &impl Fn(&Node, &str) -> bool,
        path: &str,
    ) {
        let world = node.world_matrix(parent);
        let path = format!("{path}/{}", node.name);
        if let Some(drawable) = &node.drawable {
            let mesh = &drawable.mesh;
            let indices: Vec<u32> = mesh
                .indices
                .clone()
                .unwrap_or_else(|| (0..mesh.positions.len() as u32).collect());
            let matrices = match &drawable.instances {
                Some(list) => list.iter().map(|i| world * i.matrix).collect(),
                None => vec![world],
            };
            for matrix in matrices {
                let corners: Vec<DVec3> = mesh
                    .positions
                    .iter()
                    .map(|p| {
                        matrix.transform_point3(DVec3::new(p[0].into(), p[1].into(), p[2].into()))
                    })
                    .collect();
                self.add_mesh(&corners, &indices, stones(node, &path));
            }
        }
        for child in &node.children {
            self.add(child, world, stones, &path);
        }
    }

    fn add_mesh(&mut self, corners: &[DVec3], indices: &[u32], stones: bool) {
        let mut parent: Vec<usize> = (0..corners.len()).collect();
        let mut first: HashMap<[i64; 3], usize> = HashMap::new();
        for (i, p) in corners.iter().enumerate() {
            let key = [p.x, p.y, p.z].map(|v| (v * 1e4).round() as i64);
            let j = *first.entry(key).or_insert(i);
            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
            parent[a] = b;
        }
        for t in indices.as_chunks::<3>().0 {
            for k in 1..3 {
                let (a, b) = (
                    root(&mut parent, t[0] as usize),
                    root(&mut parent, t[k] as usize),
                );
                parent[a] = b;
            }
        }
        let mut piece_of: HashMap<usize, usize> = HashMap::new();
        for t in indices.as_chunks::<3>().0 {
            let r = root(&mut parent, t[0] as usize);
            let next = self.pieces.len();
            let piece = *piece_of.entry(r).or_insert(next);
            if piece == next {
                self.pieces.push(Piece {
                    loose_stone: stones,
                    triangles: 0,
                    lo: DVec3::INFINITY,
                    hi: DVec3::NEG_INFINITY,
                    underside: Vec::new(),
                });
            }
            let triangle = [t[0], t[1], t[2]].map(|i| corners[i as usize]);
            let lo = triangle[0].min(triangle[1]).min(triangle[2]);
            let hi = triangle[0].max(triangle[1]).max(triangle[2]);
            self.pieces[piece].triangles += 1;
            self.pieces[piece].lo = self.pieces[piece].lo.min(lo);
            self.pieces[piece].hi = self.pieces[piece].hi.max(hi);
            let id = self.triangles.len();
            self.triangles.push((triangle, piece));
            // Far backdrop faces are hundreds of metres wide and hold no stones.
            if hi.x - lo.x > 400.0 || hi.z - lo.z > 400.0 {
                continue;
            }
            let ((x0, z0), (x1, z1)) = (cell(lo.x, lo.z), cell(hi.x, hi.z));
            for gx in x0.max(-200)..=x1.min(200) {
                for gz in z0.max(-200)..=z1.min(200) {
                    self.cells.entry((gx, gz)).or_default().push(id);
                }
            }
        }
        for (i, p) in corners.iter().enumerate() {
            if let Some(&piece) = piece_of.get(&root(&mut parent, i)) {
                let piece = &mut self.pieces[piece];
                if p.y <= piece.lo.y + (piece.hi.y - piece.lo.y) / 8.0 + 1e-6 {
                    piece.underside.push(*p);
                }
            }
        }
    }

    /// Heights of every other piece's surface straight above or below x/z.
    fn surfaces(&self, x: f64, z: f64, skip: usize) -> Vec<f64> {
        let Some(list) = self.cells.get(&cell(x, z)) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|&id| {
                let ([a, b, c], piece) = self.triangles[id];
                let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
                if piece == skip || det.abs() < 1e-12 {
                    return None;
                }
                let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
                let v = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
                (u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9)
                    .then_some(u * a.y + v * b.y + (1.0 - u - v) * c.y)
            })
            .collect()
    }

    /// Loose stones whose underside stands clear of every surface under it, with
    /// that clearance; a stone wholly under a surface (buried) is not hanging.
    /// Only pieces of `triangles` triangles count as stones, when given.
    fn hanging(&self, triangles: Option<usize>) -> Vec<(DVec3, f64)> {
        let mut hanging = Vec::new();
        for (i, piece) in self.pieces.iter().enumerate() {
            let size = piece.hi - piece.lo;
            if !piece.loose_stone
                || size.x.max(size.z).max(size.y) > LOOSE
                || triangles.is_some_and(|t| t != piece.triangles)
            {
                continue;
            }
            let centre = (piece.lo + piece.hi) / 2.0;
            let mut clearance = f64::INFINITY;
            let mut buried = false;
            for p in piece
                .underside
                .iter()
                .chain([&DVec3::new(centre.x, piece.lo.y, centre.z)])
            {
                for y in self.surfaces(p.x, p.z, i) {
                    if y <= piece.hi.y {
                        clearance = clearance.min((p.y - y).max(0.0));
                    } else {
                        buried = true;
                    }
                }
            }
            if clearance > CLEARANCE && !(buried && clearance.is_infinite()) {
                hanging.push((piece.lo, clearance));
            }
        }
        hanging
    }
}

/// The map's scenery and its standing cover, as the renderer places them.
fn world(map: MapId, theme: MapTheme, stones: impl Fn(&Node, &str) -> bool) -> World {
    let mut simulation = Simulation::with_seed(417.0);
    simulation.map_mode = map;
    simulation.reset(None);
    let mut world = World::default();
    world.add(build_scenery(theme).root(), DMat4::IDENTITY, &stones, "");
    for cover in &simulation.covers {
        let shape = CoverShape {
            kind: cover.kind,
            x: cover.x,
            z: cover.z,
            w: cover.w,
            d: cover.d,
            h: cover.h,
            color: cover.color,
            debris_seed: cover.debris_seed,
            timber_hits: Vec::new(),
            timber_join: None,
        };
        world.add(
            &cover_model(&shape, TreeDetail::Full, 0),
            DMat4::IDENTITY,
            &stones,
            "",
        );
    }
    world
}

#[test]
fn quarry_sandstone_stones_rest_on_the_ground_rock_or_heap_under_them() {
    let world = world(MapId::Quarry, MapTheme::Quarry, |node, _| {
        node.drawable.as_ref().is_some_and(
            |d| matches!(&d.material.effect, Effect::Custom { name, .. } if *name == SANDSTONE),
        )
    });
    let hanging = world.hanging(None);
    assert!(
        hanging.is_empty(),
        "{} stones hang in the air: {hanging:?}",
        hanging.len()
    );
}

#[test]
fn village_creek_bank_stones_rest_on_the_drawn_terrain() {
    // The bank stones are the landscape's 20-face icosahedra.
    let world = world(MapId::Village, MapTheme::Village, |_, path| {
        path.contains("pine-valley-landscape")
    });
    let hanging = world.hanging(Some(20));
    assert!(
        hanging.is_empty(),
        "{} stones hang in the air: {hanging:?}",
        hanging.len()
    );
}
