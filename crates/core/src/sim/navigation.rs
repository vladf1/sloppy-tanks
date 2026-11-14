//! Grid navigation for bots: an occupancy grid inflated by hull clearance and a
//! deterministic four-neighbor A*.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::arena::{box_turn, cover_turn};
use super::data::ARENA;
use super::structure_physics::ground_footprints;
use super::math::Vec2;
use super::tree_proportions::tree_proportions;
use super::types::{Cover, CoverKind};

/// A cell's side (m), and the standard arena's half extent and grid: a map larger than the
/// standard arena (a map scale above 1) takes as many more cells as cover it at the same
/// size, so a route keeps the same room off cover in metres (a cell's centre stands as
/// near a cover's grown box as on a standard map); a smaller map keeps the standard grid.
const CELL_SIZE: f64 = 1.5;
const HALF_ARENA: f64 = ARENA;
pub const GRID_SIZE: usize = 80; // ceil(2 * HALF_ARENA / CELL_SIZE)
const CELLS: usize = GRID_SIZE * GRID_SIZE;
// Inflate obstacles by a hull margin so a point path leaves room for the actual tank.
const HULL_CLEARANCE: f64 = 1.35;
const REBUILD_PADDING: f64 = 2.0;
const NEAREST_SEARCH_RADIUS: i64 = 9;
const DIRECTIONS: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// An axis-aligned planar footprint; w/d are full dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
}

impl From<&Cover> for Footprint {
    /// The cover's box, or a turned cover's bounds along the world's axes.
    fn from(cover: &Cover) -> Self {
        let (sin, cos) = box_turn(cover.kind, cover.house_plan).sin_cos();
        let (sin, cos) = (sin.abs(), cos.abs());
        Self {
            x: cover.x,
            z: cover.z,
            w: cover.w * cos + cover.d * sin,
            d: cover.w * sin + cover.d * cos,
        }
    }
}

/// How far apart (m) the squares that stand in for a turned cover's footprint lie along
/// it: well under the hull clearance they are grown by, so their union is the turned box.
const TURNED_STEP: f64 = 0.5;

/// A turned cover's footprint as a row of squares along its long side (Senda's yard
/// walls are long and thin, so their bounds along the world's axes would block the side
/// yards beside them).
fn turned_footprints(cover: &Cover, obstacles: &mut Vec<Footprint>) {
    let turn = cover_turn(cover.kind, cover.house_plan);
    let (sin, cos) = turn.sin_cos();
    // The box's own axes in the world (as a house's build frame turns).
    let (length, width, along) = if cover.w >= cover.d {
        (cover.w, cover.d, (cos, -sin))
    } else {
        (cover.d, cover.w, (sin, cos))
    };
    let steps = ((length - width) / TURNED_STEP).ceil().max(1.0);
    for k in 0..=steps as usize {
        let u = (k as f64 / steps - 0.5) * (length - width);
        obstacles.push(Footprint {
            x: cover.x + along.0 * u,
            z: cover.z + along.1 * u,
            w: width,
            d: width,
        });
    }
}

/// A turned box's footprint, `w` by `d` about `(x, z)` turned `turn` (radians about +y),
/// as squares [`TURNED_STEP`] across filling it: grown by the hull clearance, their union
/// is the turned box, where its bounds along the world's axes would also block the room
/// beside its corners (the Neighborhood's houses stand square to curving streets).
pub fn turned_box_footprints(
    (x, z): (f64, f64),
    (w, d): (f64, f64),
    turn: f64,
    obstacles: &mut Vec<Footprint>,
) {
    let (sin, cos) = turn.sin_cos();
    let steps = |length: f64| (length / TURNED_STEP).ceil().max(1.0) as usize;
    let (across, deep) = (steps(w), steps(d));
    for i in 0..across {
        for j in 0..deep {
            // A square's middle in the box's own axes, then turned as a house's build
            // frame turns.
            let u = ((i as f64 + 0.5) / across as f64 - 0.5) * w;
            let v = ((j as f64 + 0.5) / deep as f64 - 0.5) * d;
            obstacles.push(Footprint {
                x: x + u * cos + v * sin,
                z: z - u * sin + v * cos,
                w: w / across as f64,
                d: d / deep as f64,
            });
        }
    }
}

/// One cell's part in a search. Each field is current only while its stamp (`reached`, or
/// `closed` for the closed flag) equals the navigation's `search`, so a new search starts
/// without clearing the grid.
#[derive(Clone, Copy, Default)]
struct SearchCell {
    reached: u32,
    closed: u32,
    cost: i32,
    parent: u32,
    /// The order in which the search first reached this cell.
    order: u32,
}

pub struct Navigation {
    pub blocked: Vec<u8>,
    cells: Vec<SearchCell>,
    search: u32,
    /// The cell at each discovery order of the current search.
    discovered: Vec<u32>,
    /// Min-heap of `estimate * CELLS + discovery order`; see `find`.
    open: BinaryHeap<Reverse<u64>>,
    obstacles: Vec<Footprint>,
    pub version: u32,
    /// Cells along each side of the grid ([`Self::set_map_scale`]).
    grid: usize,
}

impl Default for Navigation {
    fn default() -> Self {
        Self::new()
    }
}

impl Navigation {
    pub fn new() -> Self {
        Self {
            blocked: vec![0; CELLS],
            cells: vec![SearchCell::default(); CELLS],
            search: 0,
            discovered: vec![0; CELLS],
            open: BinaryHeap::new(),
            obstacles: Vec::new(),
            version: 0,
            grid: GRID_SIZE,
        }
    }

    /// Lay the grid over a map `scale` times the standard arena: a larger map's grid takes
    /// as many more cells of the same size as cover it ([`CELL_SIZE`]); a standard or
    /// smaller one keeps the standard grid exactly.
    pub fn set_map_scale(&mut self, scale: f64) {
        let grid = if scale > 1.0 {
            (2.0 * HALF_ARENA * scale / CELL_SIZE).ceil() as usize
        } else {
            GRID_SIZE
        };
        self.size_grid(grid);
    }

    /// The grid `grid` cells a side, cleared.
    fn size_grid(&mut self, grid: usize) {
        self.grid = grid;
        let cells = grid * grid;
        self.blocked.clear();
        self.blocked.resize(cells, 0);
        self.cells.clear();
        self.cells.resize(cells, SearchCell::default());
        self.discovered.clear();
        self.discovered.resize(cells, 0);
    }

    /// Cells along each side of the grid.
    pub fn grid_size(&self) -> usize {
        self.grid
    }

    /// A cell's side (m).
    pub fn cell_size(&self) -> f64 {
        CELL_SIZE
    }

    /// How far (m) the grid reaches from the centre along either axis.
    pub fn half_extent(&self) -> f64 {
        self.grid as f64 * CELL_SIZE / 2.0
    }

    /// Return to the state of [`Self::new`] in place, keeping the grids' storage.
    pub fn clear(&mut self) {
        self.size_grid(GRID_SIZE);
        self.search = 0;
        self.open.clear();
        self.obstacles.clear();
        self.version = 0;
    }

    /// Recompute occupancy for the whole grid, or only the cells around `region`. Living
    /// cover blocks its footprint; a felled tree keeps blocking its stump.
    pub fn rebuild(&mut self, covers: &[Cover], region: Option<Footprint>) {
        self.obstacles.clear();
        for cover in covers {
            if cover.structure.is_some() {
                // A structure blocks where its standing parts reach the ground, so tanks
                // drive through its gaps where they fit.
                ground_footprints(cover, &mut self.obstacles);
            } else if cover.alive && cover_turn(cover.kind, cover.house_plan) != 0.0 {
                if cover.w.min(cover.d) > TURNED_STEP * 2.0 {
                    // A broad turned box (a patio's loggia) fills its own outline.
                    let turn = cover_turn(cover.kind, cover.house_plan);
                    turned_box_footprints(
                        (cover.x, cover.z),
                        (cover.w, cover.d),
                        turn,
                        &mut self.obstacles,
                    );
                } else {
                    turned_footprints(cover, &mut self.obstacles);
                }
            } else if cover.alive {
                self.obstacles.push(Footprint::from(cover));
            } else if cover.kind == CoverKind::Tree {
                let diameter = tree_proportions(cover.x, cover.z, cover.w, cover.d, cover.h)
                    .stump_radius
                    * 2.0;
                self.obstacles.push(Footprint {
                    x: cover.x,
                    z: cover.z,
                    w: diameter,
                    d: diameter,
                });
            }
        }
        self.occupy(region);
        self.version += 1;
    }

    /// Mark the cells of the grid, or of those around `region`, that an obstacle's
    /// inflated footprint covers.
    fn occupy(&mut self, region: Option<Footprint>) {
        let (grid, half) = (self.grid, self.half_extent());
        let last = grid as f64 - 1.0;
        let (x0, x1, z0, z1) = match region {
            Some(r) => (
                0f64.max(((r.x - r.w / 2.0 - REBUILD_PADDING + half) / CELL_SIZE).floor()),
                last.min(((r.x + r.w / 2.0 + REBUILD_PADDING + half) / CELL_SIZE).ceil()),
                0f64.max(((r.z - r.d / 2.0 - REBUILD_PADDING + half) / CELL_SIZE).floor()),
                last.min(((r.z + r.d / 2.0 + REBUILD_PADDING + half) / CELL_SIZE).ceil()),
            ),
            None => (0.0, last, 0.0, last),
        };
        let (x0, x1, z0, z1) = (x0 as i64, x1 as i64, z0 as i64, z1 as i64);
        for z in z0..=z1 {
            for x in x0..=x1 {
                self.blocked[(z * grid as i64 + x) as usize] = 0;
            }
        }
        // Each obstacle tests only the cells about it (a cell beyond the farthest
        // centre it can reach on each side), not every cell every obstacle.
        let index = |at: f64| ((at + half) / CELL_SIZE - 0.5).floor() as i64;
        for cover in &self.obstacles {
            let (reach_x, reach_z) = (
                cover.w / 2.0 + HULL_CLEARANCE,
                cover.d / 2.0 + HULL_CLEARANCE,
            );
            let from_x = index(cover.x - reach_x).saturating_sub(1);
            let to_x = index(cover.x + reach_x).saturating_add(1);
            let from_z = index(cover.z - reach_z).saturating_sub(1);
            let to_z = index(cover.z + reach_z).saturating_add(1);
            for z in from_z.max(z0)..=to_z.min(z1) {
                for x in from_x.max(x0)..=to_x.min(x1) {
                    let cell = (z * grid as i64 + x) as usize;
                    let position = self.point(cell);
                    if (position.x - cover.x).abs() < reach_x
                        && (position.z - cover.z).abs() < reach_z
                    {
                        self.blocked[cell] = 1;
                    }
                }
            }
        }
    }

    pub fn index(&self, position: Vec2) -> usize {
        self.cell_at(position.x, position.z)
    }

    pub fn is_blocked(&self, position: Vec2) -> bool {
        self.blocked[self.index(position)] != 0
    }

    pub fn point(&self, cell: usize) -> Vec2 {
        Vec2 {
            x: ((cell % self.grid) as f64 + 0.5) * CELL_SIZE - self.half_extent(),
            z: ((cell / self.grid) as f64 + 0.5) * CELL_SIZE - self.half_extent(),
        }
    }

    pub fn nearest(&self, cell: usize) -> usize {
        if self.blocked[cell] == 0 {
            return cell;
        }
        let size = self.grid as i64;
        for r in 1..NEAREST_SEARCH_RADIUS {
            for z in -r..=r {
                for x in -r..=r {
                    let a = (cell % self.grid) as i64 + x;
                    let b = (cell / self.grid) as i64 + z;
                    if a >= 0
                        && a < size
                        && b >= 0
                        && b < size
                        && self.blocked[(b * size + a) as usize] == 0
                    {
                        return (b * size + a) as usize;
                    }
                }
            }
        }
        cell
    }

    /// Conservative grid visibility for shortening routes without cutting corners.
    pub fn clear_line(&self, from: Vec2, to: Vec2) -> bool {
        let steps = ((to.x - from.x).hypot(to.z - from.z) / (CELL_SIZE / 3.0)).ceil() as i64;
        for i in 0..=steps {
            let f = if steps != 0 {
                i as f64 / steps as f64
            } else {
                0.0
            };
            if self.blocked[self.cell_at(from.x + (to.x - from.x) * f, from.z + (to.z - from.z) * f)]
                != 0
            {
                return false;
            }
        }
        true
    }

    /// The allocating form of [`Self::find_into`].
    pub fn find(&mut self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let mut path = Vec::new();
        self.find_into(from, to, &mut path);
        path
    }

    /// Replace a route while retaining its storage, including when no route is reachable.
    ///
    /// Four-neighbor A*: Manhattan distance is admissible. The open list is a heap ordered by
    /// estimate, then by the order cells were first reached, which is the order a linear scan
    /// of an append-only list picks among equal estimates; seeded routes depend on that order.
    pub fn find_into(&mut self, from: Vec2, to: Vec2, path: &mut Vec<Vec2>) {
        path.clear();
        let start = self.nearest(self.index(from));
        let goal = self.nearest(self.index(to));
        self.search = self.search.wrapping_add(1);
        if self.search == 0 {
            self.cells.fill(SearchCell::default());
            self.search = 1;
        }
        let search = self.search;
        self.open.clear();
        let grid = self.grid;
        let cells = (grid * grid) as u64;
        let goal_x = (goal % grid) as i64;
        let goal_z = (goal / grid) as i64;
        let heuristic = |cell: usize| {
            ((cell % grid) as i64 - goal_x).abs() + ((cell / grid) as i64 - goal_z).abs()
        };
        // A cheaper route to an open cell pushes a smaller estimate with the cell's original
        // discovery order, so the outdated entry always pops after the cell has closed.
        let mut discoveries = 0u32;
        let mut reach = |nav: &mut Navigation, cell: usize, cost: i32, parent: usize| {
            let state = &mut nav.cells[cell];
            if state.reached != search {
                state.reached = search;
                state.order = discoveries;
                nav.discovered[discoveries as usize] = cell as u32;
                discoveries += 1;
            }
            state.cost = cost;
            state.parent = parent as u32;
            nav.open.push(Reverse(
                (cost as i64 + heuristic(cell)) as u64 * cells + state.order as u64,
            ));
        };
        reach(self, start, 0, start);
        let mut reached = start;
        while let Some(Reverse(key)) = self.open.pop() {
            let current = self.discovered[(key % cells) as usize] as usize;
            let state = &mut self.cells[current];
            if state.closed == search {
                continue;
            }
            state.closed = search;
            let cost = state.cost + 1;
            if current == goal {
                reached = current;
                break;
            }
            let x = (current % grid) as i64;
            let z = (current / grid) as i64;
            for (dx, dz) in DIRECTIONS {
                let nx = x + dx;
                let nz = z + dz;
                if nx < 0 || nz < 0 || nx >= grid as i64 || nz >= grid as i64 {
                    continue;
                }
                let next = (nz * grid as i64 + nx) as usize;
                if self.blocked[next] != 0 {
                    continue;
                }
                let neighbor = &self.cells[next];
                if neighbor.closed == search {
                    continue;
                }
                if neighbor.reached != search || cost < neighbor.cost {
                    reach(self, next, cost, current);
                }
            }
        }
        if reached != goal {
            return;
        }
        while reached != start {
            path.push(self.point(reached));
            reached = self.cells[reached].parent as usize;
        }
        path.reverse();
    }

    fn cell_at(&self, x: f64, z: f64) -> usize {
        let (last, half) = (self.grid as f64 - 1.0, self.half_extent());
        let column = 0f64.max(last.min(((x + half) / CELL_SIZE).floor()));
        let row = 0f64.max(last.min(((z + half) / CELL_SIZE).floor()));
        row as usize * self.grid + column as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_survive_the_search_stamp_wrapping_around() {
        let mut fresh = Navigation::new();
        let mut wrapping = Navigation::new();
        for z in 20..60 {
            fresh.blocked[z * GRID_SIZE + 40] = 1;
            wrapping.blocked[z * GRID_SIZE + 40] = 1;
        }
        wrapping.search = u32::MAX - 1;
        let routes = [
            (Vec2::new(-20.0, 0.0), Vec2::new(20.0, 0.0)),
            (Vec2::new(20.0, 5.0), Vec2::new(-20.0, -5.0)),
            (Vec2::new(-20.0, 0.0), Vec2::new(20.0, 0.0)),
            (Vec2::new(0.0, -50.0), Vec2::new(5.0, 50.0)),
        ];
        for (from, to) in routes {
            let expected = fresh.find(from, to);
            assert!(!expected.is_empty());
            assert_eq!(wrapping.find(from, to), expected);
        }
        assert!(wrapping.search < 4, "the stamp wrapped during the routes");
    }

    /// Every cell any obstacle covers, testing every obstacle at every cell.
    fn occupied_everywhere(obstacles: &[Footprint], nav: &Navigation) -> Vec<u8> {
        (0..CELLS)
            .map(|cell| {
                let position = nav.point(cell);
                obstacles.iter().any(|cover| {
                    (position.x - cover.x).abs() < cover.w / 2.0 + HULL_CLEARANCE
                        && (position.z - cover.z).abs() < cover.d / 2.0 + HULL_CLEARANCE
                }) as u8
            })
            .collect()
    }

    #[test]
    fn occupancy_matches_testing_every_obstacle_at_every_cell() {
        // Footprints of every size, some on cell edges and centres, some reaching past
        // the arena, and degenerate ones.
        let mut random = super::super::math::Random::new(7.0);
        let mut obstacles: Vec<Footprint> = (0..300)
            .map(|_| Footprint {
                x: (random.next() - 0.5) * 140.0,
                z: (random.next() - 0.5) * 140.0,
                w: random.next() * random.next() * 30.0,
                d: random.next() * 4.0,
            })
            .collect();
        obstacles.extend([
            Footprint { x: 0.0, z: 0.0, w: 0.0, d: 0.0 },
            Footprint { x: 1.5, z: -0.75, w: 0.3, d: 3.0 },
            Footprint { x: -60.0, z: 60.0, w: 2.0, d: 2.0 },
            Footprint { x: 10.0, z: 5.0, w: f64::NAN, d: 1.0 },
            Footprint { x: 0.0, z: 20.0, w: f64::INFINITY, d: 1.0 },
        ]);
        let mut nav = Navigation::new();
        nav.obstacles = obstacles.clone();
        nav.occupy(None);
        let expected = occupied_everywhere(&obstacles, &nav);
        assert_eq!(nav.blocked, expected);
        assert!(expected.contains(&0) && expected.contains(&1));
        // A region rebuild clears and marks only its cells.
        nav.blocked.fill(1);
        let region = Footprint { x: 12.0, z: -7.0, w: 9.0, d: 5.0 };
        nav.occupy(Some(region));
        let changed: Vec<usize> = (0..CELLS).filter(|&cell| nav.blocked[cell] != expected[cell]).collect();
        let region_cells = |cell: usize| {
            let p = nav.point(cell);
            (p.x - region.x).abs() < region.w / 2.0 + REBUILD_PADDING
                && (p.z - region.z).abs() < region.d / 2.0 + REBUILD_PADDING
        };
        assert!(changed.iter().all(|&cell| !region_cells(cell)), "{changed:?}");
        assert!(
            (0..CELLS).filter(|&cell| region_cells(cell)).any(|cell| expected[cell] == 0),
            "the region holds an open cell"
        );
    }
}
