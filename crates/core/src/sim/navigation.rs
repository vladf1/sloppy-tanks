//! Grid navigation for bots: an occupancy grid inflated by hull clearance and a
//! deterministic four-neighbor A*.

use super::data::ARENA;
use super::math::Vec2;
use super::tree_proportions::tree_proportions;
use super::types::{Cover, CoverKind};

const CELL_SIZE: f64 = 1.5;
const HALF_ARENA: f64 = ARENA;
pub const GRID_SIZE: usize = 80; // ceil(2 * HALF_ARENA / CELL_SIZE)
const CELLS: usize = GRID_SIZE * GRID_SIZE;
// Inflate obstacles by a hull margin so a point path leaves room for the actual tank.
const HULL_CLEARANCE: f64 = 1.35;
const REBUILD_PADDING: f64 = 2.0;
const NEAREST_SEARCH_RADIUS: i64 = 9;
const DIRECTIONS: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const UNREACHED: i32 = i32::MAX;

/// An axis-aligned planar footprint; w/d are full dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
}

impl From<&Cover> for Footprint {
    fn from(cover: &Cover) -> Self {
        Self {
            x: cover.x,
            z: cover.z,
            w: cover.w,
            d: cover.d,
        }
    }
}

pub struct Navigation {
    pub blocked: Vec<u8>,
    costs: Vec<i32>,
    parent: Vec<i32>,
    closed: Vec<u8>,
    /// Discovery order of each cell in the current search, and the cell at each order.
    order: Vec<u32>,
    discovered: Vec<u32>,
    /// Min-heap of `estimate * CELLS + discovery order`; see `find`.
    open: Vec<u64>,
    obstacles: Vec<Footprint>,
    pub version: u32,
    pub paths: u32,
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
            costs: vec![UNREACHED; CELLS],
            parent: vec![-1; CELLS],
            closed: vec![0; CELLS],
            order: vec![0; CELLS],
            discovered: vec![0; CELLS],
            open: Vec::new(),
            obstacles: Vec::new(),
            version: 0,
            paths: 0,
        }
    }

    /// Recompute occupancy for the whole grid, or only the cells around `region`. Living
    /// cover blocks its footprint; a felled tree keeps blocking its stump.
    pub fn rebuild(&mut self, covers: &[Cover], region: Option<Footprint>) {
        self.obstacles.clear();
        for cover in covers {
            if cover.alive {
                self.obstacles.push(Footprint::from(cover));
            } else if cover.kind == CoverKind::Tree {
                let diameter = tree_proportions(cover.x, cover.z, cover.w, cover.d, cover.h).stump_radius * 2.0;
                self.obstacles.push(Footprint {
                    x: cover.x,
                    z: cover.z,
                    w: diameter,
                    d: diameter,
                });
            }
        }
        let last = GRID_SIZE as f64 - 1.0;
        let (x0, x1, z0, z1) = match region {
            Some(r) => (
                0f64.max(((r.x - r.w / 2.0 - REBUILD_PADDING + HALF_ARENA) / CELL_SIZE).floor()),
                last.min(((r.x + r.w / 2.0 + REBUILD_PADDING + HALF_ARENA) / CELL_SIZE).ceil()),
                0f64.max(((r.z - r.d / 2.0 - REBUILD_PADDING + HALF_ARENA) / CELL_SIZE).floor()),
                last.min(((r.z + r.d / 2.0 + REBUILD_PADDING + HALF_ARENA) / CELL_SIZE).ceil()),
            ),
            None => (0.0, last, 0.0, last),
        };
        let (x0, x1, z0, z1) = (x0 as i64, x1 as i64, z0 as i64, z1 as i64);
        for z in z0..=z1 {
            for x in x0..=x1 {
                let cell = (z * GRID_SIZE as i64 + x) as usize;
                let position = self.point(cell);
                self.blocked[cell] = self.obstacles.iter().any(|cover| {
                    (position.x - cover.x).abs() < cover.w / 2.0 + HULL_CLEARANCE
                        && (position.z - cover.z).abs() < cover.d / 2.0 + HULL_CLEARANCE
                }) as u8;
            }
        }
        self.version += 1;
    }

    pub fn index(&self, position: Vec2) -> usize {
        cell_at(position.x, position.z)
    }

    pub fn is_blocked(&self, position: Vec2) -> bool {
        self.blocked[self.index(position)] != 0
    }

    pub fn point(&self, cell: usize) -> Vec2 {
        Vec2 {
            x: ((cell % GRID_SIZE) as f64 + 0.5) * CELL_SIZE - HALF_ARENA,
            z: ((cell / GRID_SIZE) as f64 + 0.5) * CELL_SIZE - HALF_ARENA,
        }
    }

    pub fn nearest(&self, cell: usize) -> usize {
        if self.blocked[cell] == 0 {
            return cell;
        }
        let size = GRID_SIZE as i64;
        for r in 1..NEAREST_SEARCH_RADIUS {
            for z in -r..=r {
                for x in -r..=r {
                    let a = (cell % GRID_SIZE) as i64 + x;
                    let b = (cell / GRID_SIZE) as i64 + z;
                    if a >= 0 && a < size && b >= 0 && b < size && self.blocked[(b * size + a) as usize] == 0 {
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
            let f = if steps != 0 { i as f64 / steps as f64 } else { 0.0 };
            if self.blocked[cell_at(from.x + (to.x - from.x) * f, from.z + (to.z - from.z) * f)] != 0 {
                return false;
            }
        }
        true
    }

    /// Four-neighbor A*: Manhattan distance is admissible. The open list is a heap ordered by
    /// estimate, then by the order cells were first reached, which is the order a linear scan
    /// of an append-only list picks among equal estimates; seeded routes depend on that order.
    pub fn find(&mut self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        self.paths += 1;
        let start = self.nearest(self.index(from));
        let goal = self.nearest(self.index(to));
        self.costs.fill(UNREACHED);
        self.parent.fill(-1);
        self.closed.fill(0);
        self.open.clear();
        let goal_x = (goal % GRID_SIZE) as i64;
        let goal_z = (goal / GRID_SIZE) as i64;
        let heuristic =
            |cell: usize| ((cell % GRID_SIZE) as i64 - goal_x).abs() + ((cell / GRID_SIZE) as i64 - goal_z).abs();
        // A cheaper route to an open cell pushes a smaller estimate with the cell's original
        // discovery order, so the outdated entry always pops after the cell has closed.
        let mut discoveries = 0u32;
        let mut reach = |nav: &mut Navigation, cell: usize, cost: i32| {
            if nav.costs[cell] == UNREACHED {
                nav.order[cell] = discoveries;
                nav.discovered[discoveries as usize] = cell as u32;
                discoveries += 1;
            }
            nav.costs[cell] = cost;
            push_heap(
                &mut nav.open,
                (cost as i64 + heuristic(cell)) as u64 * CELLS as u64 + nav.order[cell] as u64,
            );
        };
        reach(self, start, 0);
        let mut reached = start;
        while let Some(key) = pop_heap(&mut self.open) {
            let current = self.discovered[(key % CELLS as u64) as usize] as usize;
            if self.closed[current] != 0 {
                continue;
            }
            self.closed[current] = 1;
            if current == goal {
                reached = current;
                break;
            }
            let x = (current % GRID_SIZE) as i64;
            let z = (current / GRID_SIZE) as i64;
            for (dx, dz) in DIRECTIONS {
                let nx = x + dx;
                let nz = z + dz;
                if nx < 0 || nz < 0 || nx >= GRID_SIZE as i64 || nz >= GRID_SIZE as i64 {
                    continue;
                }
                let next = (nz * GRID_SIZE as i64 + nx) as usize;
                if self.blocked[next] != 0 || self.closed[next] != 0 {
                    continue;
                }
                let cost = self.costs[current] + 1;
                if cost < self.costs[next] {
                    self.parent[next] = current as i32;
                    reach(self, next, cost);
                }
            }
        }
        if reached != goal {
            return Vec::new();
        }
        let mut path = Vec::new();
        while reached != start {
            path.push(self.point(reached));
            reached = self.parent[reached] as usize;
        }
        path.reverse();
        path
    }
}

fn cell_at(x: f64, z: f64) -> usize {
    let last = GRID_SIZE as f64 - 1.0;
    let column = 0f64.max(last.min(((x + HALF_ARENA) / CELL_SIZE).floor()));
    let row = 0f64.max(last.min(((z + HALF_ARENA) / CELL_SIZE).floor()));
    row as usize * GRID_SIZE + column as usize
}

fn push_heap(heap: &mut Vec<u64>, key: u64) {
    let mut i = heap.len();
    heap.push(key);
    while i > 0 {
        let up = (i - 1) >> 1;
        if heap[up] <= key {
            break;
        }
        heap[i] = heap[up];
        i = up;
    }
    heap[i] = key;
}

fn pop_heap(heap: &mut Vec<u64>) -> Option<u64> {
    let top = *heap.first()?;
    let last = heap.pop()?;
    if !heap.is_empty() {
        let mut i = 0;
        loop {
            let mut child = 2 * i + 1;
            if child >= heap.len() {
                break;
            }
            if child + 1 < heap.len() && heap[child + 1] < heap[child] {
                child += 1;
            }
            if heap[child] >= last {
                break;
            }
            heap[i] = heap[child];
            i = child;
        }
        heap[i] = last;
    }
    Some(top)
}
