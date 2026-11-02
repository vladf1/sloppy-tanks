//! The solid cover effects must respect: what a shell struck when its impact names
//! no cover (a steel container or stone), the face it struck, and the boxes smoke
//! billows roll round rather than slice through.
//!
//! Smoke puffs are camera-facing cards. One that overlaps a container or a butte
//! is cut by it along a hard straight line, and one just behind it, pulled toward
//! the eye, draws over its edge. Billows beside a wall therefore keep clear of the
//! wall's box, the few wedged where no push frees them thin out, and the walls
//! most smoke gathers round go to the puff shader (`soft_boxes`), which thins
//! every billow toward their faces like a soft particle, so none is cut.

use glam::Vec3;
use sloppy_core::sim::house_layout::{HouseDesign, WallFinish};
use sloppy_core::sim::{CoverKind, RenderState};

use super::pool::PoolBuffer;

/// A shell's impact point lies on a struck wall's face, or within this much of it (m).
const STRIKE_REACH: f64 = 0.7;
/// The radius a wall mark would like round its centre (m): the strike moves its
/// point up to this far along a narrow face to give the mark room.
const MARK_RADIUS: f64 = 0.65;
/// A puff keeps its centre this share of its card's radius clear of a wall's box,
/// and thins to `SQUEEZED` where a second wall leaves it no room.
const CLEARANCE: f32 = 0.6;
const SQUEEZED: f32 = 0.35;
/// The puff shader softens billows against this many walls (`puff.wgsl`), the
/// ones with most smoke within this share of a puff's radius of them.
pub const SOFT_WALLS: usize = 3;
const SOFT_REACH: f32 = 1.3;

/// What a wall is made of, for the look of a shell striking it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallMaterial {
    /// A shipping container or tank trap: painted sheet steel.
    Steel,
    /// Concrete, rock and masonry.
    Stone,
    /// A cottage's painted siding over its framing and plaster: the model shows
    /// each shell's hole, so a hit leaves no crater mark, and it bursts into
    /// painted boards, splinters and pale plaster dust.
    Siding,
}

/// A brick cottage's walls, for the chips a hit knocks off them.
const BRICK: u32 = 0x8e5240;

/// One solid cover's axis-aligned box (its navigation footprint): centre, half
/// extents and height.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WallBox {
    x: f64,
    z: f64,
    half_w: f64,
    half_d: f64,
    height: f64,
    /// `None` for cover with its own hit effects (crates, timber), which smoke
    /// still rolls round.
    material: Option<WallMaterial>,
    /// Whether a shell's crater mark may stand on its faces (`Strike::marked`).
    marked: bool,
    /// Its paint or stone, sRGB hex.
    color: u32,
    /// A cottage's trim paint; the wall's color on other cover.
    trim: u32,
    /// A cottage's roof paint; the wall's color on other cover.
    roof: u32,
}

/// A standing cottage's box and paints, for the boards its collapse blows out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HouseBox {
    pub center: Vec3,
    pub half_w: f32,
    pub half_d: f32,
    pub height: f32,
    /// Siding, trim and roof paint, sRGB hex.
    pub paints: [u32; 3],
}

/// Where a shell struck a wall: the point on its face, the face's outward normal
/// and how much room the face leaves round the point for a mark (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub material: WallMaterial,
    pub point: Vec3,
    pub normal: Vec3,
    pub room: f32,
    /// Whether the hit leaves a crater mark on the face. The mark is a flat decal on
    /// the box, fixed where it struck: a cottage's model shows each hole itself, and
    /// dragon's teeth and hedgehogs are neither boxes nor still (shells push them),
    /// so a mark there hung in the air beside their slopes and beams.
    pub marked: bool,
    /// The wall's paint or stone, sRGB hex.
    pub color: u32,
    /// A cottage's trim paint; the wall's color on other cover.
    pub trim: u32,
}

/// The current round's solid cover, refreshed every frame from the render state
/// into a buffer that keeps its allocation.
#[derive(Clone, Debug, Default)]
pub struct Walls {
    boxes: Vec<WallBox>,
    /// How much smoke gathers round each box this frame.
    smoke: Vec<f32>,
    /// The puff shader's walls: up to `SOFT_WALLS` boxes as (centre x, centre z,
    /// half width, half depth), then their heights and the count.
    soft: [[f32; 4]; 4],
    /// The boxes by ground cell, so a puff tests the few walls near it.
    grid: WallGrid,
    /// Scratch: one puff's candidate walls (as a bit set, then listed) and the smoke
    /// it leaves at them.
    mask: Vec<u64>,
    candidates: Vec<u32>,
    gathered: Vec<(u32, f32)>,
}

/// A wall box in the puffs' single precision: centre x and z, half width and depth,
/// height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PuffBox {
    x: f32,
    z: f32,
    half_w: f32,
    half_d: f32,
    height: f32,
}

/// Square ground cells holding the boxes that overlap them (`WALL_CELL` metres, at
/// most `MAX_GRID_CELLS` along a side, widened for a larger yard).
const WALL_CELL: f32 = 8.0;
const MAX_GRID_CELLS: usize = 64;

#[derive(Clone, Debug, Default)]
struct WallGrid {
    /// The walls it was built for; most frames the round's cover stands as it did.
    source: Vec<WallBox>,
    boxes: Vec<PuffBox>,
    origin: [f32; 2],
    cell: f32,
    columns: usize,
    rows: usize,
    /// 64-bit words in a cell's set of boxes.
    words: usize,
    /// Cell `c`'s boxes as a bit set, box `i` at bit `i % 64` of word
    /// `bits[c * words + i / 64]`: a puff unions its cells' sets and reads the boxes
    /// out in index order, where gathering lists and sorting them cost twice as much.
    bits: Vec<u64>,
}

impl WallGrid {
    fn build(&mut self, boxes: &[WallBox]) {
        if self.source == boxes {
            return;
        }
        self.source.clear();
        self.source.extend_from_slice(boxes);
        self.boxes.clear();
        self.boxes.extend(boxes.iter().map(|wall| PuffBox {
            x: wall.x as f32,
            z: wall.z as f32,
            half_w: wall.half_w as f32,
            half_d: wall.half_d as f32,
            height: wall.height as f32,
        }));
        let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for b in &self.boxes {
            min = [min[0].min(b.x - b.half_w), min[1].min(b.z - b.half_d)];
            max = [max[0].max(b.x + b.half_w), max[1].max(b.z + b.half_d)];
        }
        let span = (max[0] - min[0]).max(max[1] - min[1]).max(0.0);
        // The far edge's cell is one past `span / cell`, still inside the grid.
        self.cell = WALL_CELL.max(span / (MAX_GRID_CELLS - 1) as f32);
        self.origin = min;
        let count = |extent: f32| ((extent / self.cell).floor() as usize + 1).min(MAX_GRID_CELLS);
        self.columns = count(max[0] - min[0]);
        self.rows = count(max[1] - min[1]);
        self.words = self.boxes.len().div_ceil(64);
        self.bits.clear();
        self.bits.resize(self.columns * self.rows * self.words, 0);
        for (index, b) in self.boxes.iter().enumerate() {
            let (columns, rows) = self.cells(b, 0.0);
            for row in rows {
                for column in columns.clone() {
                    let cell = row * self.columns + column;
                    self.bits[cell * self.words + index / 64] |= 1 << (index % 64);
                }
            }
        }
    }

    /// The columns and rows of the cells `b`, grown by `margin`, overlaps.
    fn cells(&self, b: &PuffBox, margin: f32) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let span = |center: f32, half: f32, origin: f32, cells: usize| {
            let low = ((center - half - margin - origin) / self.cell).floor();
            let high = ((center + half + margin - origin) / self.cell).floor();
            if high < 0.0 || low >= cells as f32 {
                return 0..0;
            }
            (low.max(0.0) as usize)..(high as usize + 1).min(cells)
        };
        (
            span(b.x, b.half_w, self.origin[0], self.columns),
            span(b.z, b.half_d, self.origin[1], self.rows),
        )
    }

    /// The boxes overlapping the square of half size `reach` round (x, z), each once
    /// and in index order (`mask` is scratch).
    fn near(&self, x: f32, z: f32, reach: f32, mask: &mut Vec<u64>, out: &mut Vec<u32>) {
        out.clear();
        let point = PuffBox {
            x,
            z,
            ..PuffBox::default()
        };
        let (columns, rows) = self.cells(&point, reach);
        if columns.is_empty() || rows.is_empty() {
            return;
        }
        mask.clear();
        mask.resize(self.words, 0);
        for row in rows {
            for column in columns.clone() {
                let cell = (row * self.columns + column) * self.words;
                for (word, bits) in mask.iter_mut().zip(&self.bits[cell..cell + self.words]) {
                    *word |= bits;
                }
            }
        }
        for (word, &bits) in mask.iter().enumerate() {
            let mut bits = bits;
            while bits != 0 {
                out.push((word * 64) as u32 + bits.trailing_zeros());
                bits &= bits - 1;
            }
        }
    }
}

/// How far a puff's search for walls reaches past its softening reach, in shares of
/// its radius: room for two pushes clear of a wall. A puff the pushes carry further
/// is pushed again against every wall.
const SEARCH_SLACK: f32 = 2.0 * CLEARANCE;

/// Where one puff ends up among the walls `order` lists (every wall when `None`),
/// tested in that order (`Walls::keep_puffs_clear`), and how many pushed it;
/// `smoke(wall, weight)` takes the smoke it leaves at each. Also returns the farthest
/// the pushes carried it along either ground axis.
fn push_puff(
    start: Vec3,
    radius: f32,
    weight: f32,
    boxes: &[PuffBox],
    order: Option<&[u32]>,
    smoke: &mut dyn FnMut(u32, f32),
) -> (Vec3, u32, f32) {
    let mut center = start;
    let clearance = radius * CLEARANCE;
    let reach = radius * SOFT_REACH;
    let mut pushes = 0;
    let mut drift: f32 = 0.0;
    let count = order.map_or(boxes.len(), <[u32]>::len);
    for k in 0..count {
        let index = order.map_or(k as u32, |order| order[k]);
        let wall = boxes[index as usize];
        let (hw, hd, h) = (wall.half_w, wall.half_d, wall.height);
        let local = Vec3::new(center.x - wall.x, center.y, center.z - wall.z);
        if local.x.abs() >= hw + reach || local.z.abs() >= hd + reach || local.y >= h + reach {
            continue;
        }
        let nearest = Vec3::new(
            local.x.clamp(-hw, hw),
            local.y.clamp(0.0, h),
            local.z.clamp(-hd, hd),
        );
        let away = local - nearest;
        let distance = away.length();
        if distance < reach {
            smoke(index, weight);
        }
        if distance >= clearance {
            continue;
        }
        center += if distance > 1e-4 {
            away / distance * (clearance - distance)
        } else {
            // Buried in the box: out through the nearest side or the top.
            let (depth, normal) = [
                (hw - local.x, Vec3::X),
                (hw + local.x, Vec3::NEG_X),
                (hd - local.z, Vec3::Z),
                (hd + local.z, Vec3::NEG_Z),
                (h - local.y, Vec3::Y),
            ]
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap_or((0.0, Vec3::Y));
            normal * (depth + clearance)
        };
        pushes += 1;
        drift = drift.max((center.x - start.x).abs().max((center.z - start.z).abs()));
    }
    (center, pushes, drift)
}

impl Walls {
    pub fn sync(&mut self, state: &RenderState) {
        self.boxes.clear();
        for cover in state.covers.iter().filter(|cover| cover.alive) {
            let material = match cover.kind {
                CoverKind::Container | CoverKind::Hedgehog => Some(WallMaterial::Steel),
                CoverKind::Rock | CoverKind::Concrete | CoverKind::Teeth | CoverKind::Rubble => {
                    Some(WallMaterial::Stone)
                }
                CoverKind::House => Some(WallMaterial::Siding),
                CoverKind::Cargo | CoverKind::Timber => None,
                _ => continue,
            };
            let (color, trim, roof) = if cover.kind == CoverKind::House {
                let design = HouseDesign::of(cover.x, cover.z, cover.w, cover.d, cover.h);
                let walls = match design.style.walls {
                    WallFinish::Clapboard => design.style.siding,
                    WallFinish::Brick => BRICK,
                };
                (walls, design.style.trim, design.style.roof)
            } else {
                (cover.color, cover.color, cover.color)
            };
            self.boxes.push(WallBox {
                x: cover.x,
                z: cover.z,
                half_w: cover.w * 0.5,
                half_d: cover.d * 0.5,
                height: cover.h,
                material,
                marked: cover.kind != CoverKind::House && !cover.kind.movable(),
                color,
                trim,
                roof,
            });
        }
    }

    pub fn clear(&mut self) {
        self.boxes.clear();
        self.soft = [[0.0; 4]; 4];
    }

    /// The walls the puff shader softens smoke against this frame.
    pub fn soft_boxes(&self) -> [[f32; 4]; 4] {
        self.soft
    }

    /// The standing cottage whose footprint holds `(x, z)`, as of the last sync.
    pub fn house_at(&self, x: f64, z: f64) -> Option<HouseBox> {
        self.boxes
            .iter()
            .find(|wall| {
                wall.material == Some(WallMaterial::Siding)
                    && (x - wall.x).abs() <= wall.half_w
                    && (z - wall.z).abs() <= wall.half_d
            })
            .map(|wall| HouseBox {
                center: Vec3::new(wall.x as f32, 0.0, wall.z as f32),
                half_w: wall.half_w as f32,
                half_d: wall.half_d as f32,
                height: wall.height as f32,
                paints: [wall.color, wall.trim, wall.roof],
            })
    }

    /// The hard wall a shell struck at `(x, y, z)`: steel or stone, the nearest face
    /// and the room around the point.
    pub fn strike(&self, x: f64, y: f64, z: f64) -> Option<Strike> {
        let mut best: Option<(f64, Strike)> = None;
        for wall in &self.boxes {
            let Some(material) = wall.material else {
                continue;
            };
            let (dx, dz) = (x - wall.x, z - wall.z);
            let out_x = dx.abs() - wall.half_w;
            let out_z = dz.abs() - wall.half_d;
            let outside = out_x.max(0.0).hypot(out_z.max(0.0));
            if outside > STRIKE_REACH || best.is_some_and(|(distance, _)| outside >= distance) {
                continue;
            }
            // The face the point is nearest: the one it stands furthest out from.
            let x_face = out_x > out_z;
            let normal = if x_face {
                Vec3::new(dx.signum() as f32, 0.0, 0.0)
            } else {
                Vec3::new(0.0, 0.0, dz.signum() as f32)
            };
            // On the face, moved in from its edges far enough to give a mark room
            // (to its middle on a face too small for it).
            let fit = |value: f64, low: f64, high: f64| {
                if high - low >= 2.0 * MARK_RADIUS {
                    value.clamp(low + MARK_RADIUS, high - MARK_RADIUS)
                } else {
                    (low + high) * 0.5
                }
            };
            let height = wall.height.max(0.3);
            let y = fit(y, 0.0, height);
            let (px, pz) = if x_face {
                (
                    wall.half_w * dx.signum(),
                    fit(dz, -wall.half_d, wall.half_d),
                )
            } else {
                (
                    fit(dx, -wall.half_w, wall.half_w),
                    wall.half_d * dz.signum(),
                )
            };
            let point = Vec3::new((wall.x + px) as f32, y as f32, (wall.z + pz) as f32);
            // Room to either side along the face, and up and down it.
            let along = if x_face {
                wall.half_d - pz.abs()
            } else {
                wall.half_w - px.abs()
            };
            let room = along.min(y).min(height - y).max(0.0) as f32;
            best = Some((
                outside,
                Strike {
                    material,
                    point,
                    normal,
                    room,
                    marked: wall.marked,
                    color: wall.color,
                    trim: wall.trim,
                },
            ));
        }
        best.map(|(_, strike)| strike)
    }

    /// Push every smoke puff in `puffs` out of the walls' boxes, so a billow rolls
    /// round a container or a butte instead of being sliced by it, and thin the
    /// ones wedged where no push frees them. Each puff's new place depends only on
    /// where it would have been, so a drifting billow glides along a wall.
    ///
    /// A puff tests only the walls in the ground cells round it, in the same order as
    /// the whole list: a wall farther than its search reaches cannot touch it unless
    /// the pushes carried it that far, and then it is pushed again against every wall.
    pub fn keep_puffs_clear(&mut self, puffs: &mut PoolBuffer) {
        self.soft = [[0.0; 4]; 4];
        if self.boxes.is_empty() || puffs.is_empty() {
            return;
        }
        self.smoke.clear();
        self.smoke.resize(self.boxes.len(), 0.0);
        self.grid.build(&self.boxes);
        let Self {
            grid,
            smoke,
            mask,
            candidates,
            gathered,
            ..
        } = self;
        for puff in puffs.records_mut() {
            let start = puff.translation();
            let radius = Vec3::from_slice(&puff.world_rows[0][..3]).length();
            let weight = puff.tint[3] * radius * radius;
            let slack = radius * SEARCH_SLACK;
            grid.near(
                start.x,
                start.z,
                radius * SOFT_REACH + slack,
                mask,
                candidates,
            );
            gathered.clear();
            let (mut center, mut pushes, drift) = push_puff(
                start,
                radius,
                weight,
                &grid.boxes,
                Some(candidates),
                &mut |index, weight| gathered.push((index, weight)),
            );
            if drift < slack {
                for &(index, weight) in gathered.iter() {
                    smoke[index as usize] += weight;
                }
            } else {
                (center, pushes, _) = push_puff(
                    start,
                    radius,
                    weight,
                    &grid.boxes,
                    None,
                    &mut |index, weight| smoke[index as usize] += weight,
                );
            }
            if pushes == 0 {
                continue;
            }
            for (row, value) in puff.world_rows.iter_mut().zip(center.to_array()) {
                row[3] = value;
            }
            // A second wall's push may undo the first's: a billow wedged between two
            // boxes thins rather than slicing into either.
            if pushes > 1 {
                puff.tint[3] *= SQUEEZED;
            }
        }
        // The smokiest walls, in order.
        let mut count = 0;
        while count < SOFT_WALLS {
            let Some((index, _)) = self
                .smoke
                .iter()
                .enumerate()
                .filter(|(_, smoke)| **smoke > 0.0)
                .max_by(|a, b| a.1.total_cmp(b.1))
            else {
                break;
            };
            self.smoke[index] = 0.0;
            let wall = self.boxes[index];
            self.soft[count] = [
                wall.x as f32,
                wall.z as f32,
                wall.half_w as f32,
                wall.half_d as f32,
            ];
            self.soft[3][count] = wall.height as f32;
            count += 1;
        }
        self.soft[3][3] = count as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::pool::record;
    use glam::Mat4;

    fn container() -> Walls {
        Walls {
            boxes: vec![WallBox {
                x: 0.0,
                z: 0.0,
                half_w: 3.0,
                half_d: 1.2,
                height: 2.6,
                material: Some(WallMaterial::Steel),
                marked: true,
                color: 0x2f8a6a,
                trim: 0x2f8a6a,
                roof: 0x2f8a6a,
            }],
            ..Walls::default()
        }
    }

    #[test]
    fn a_strike_finds_the_face_its_point_lies_on() {
        let walls = container();
        let strike = walls.strike(0.5, 1.0, 1.25).expect("on the face");
        assert_eq!(strike.material, WallMaterial::Steel);
        assert_eq!(strike.normal, Vec3::Z);
        assert!((strike.point.z - 1.2).abs() < 1e-5);
        assert!((strike.room - 1.0).abs() < 1e-5);
        // Near a corner the mark moves in along the face to fit.
        let corner = walls.strike(2.95, 1.0, 1.25).expect("on the face");
        assert!((corner.point.x - (3.0 - 0.65)).abs() < 1e-5);
        assert!(walls.strike(0.0, 1.0, 4.0).is_none());
    }

    #[test]
    fn only_still_box_cover_takes_a_crater_mark() {
        let cover = |kind, x| sloppy_core::sim::render_state::RenderCover {
            kind,
            x,
            w: 1.9,
            h: 1.9,
            d: 1.9,
            alive: true,
            ..Default::default()
        };
        let mut walls = Walls::default();
        walls.sync(&RenderState {
            covers: vec![
                cover(CoverKind::Concrete, 0.0),
                cover(CoverKind::Teeth, 10.0),
                cover(CoverKind::Hedgehog, 20.0),
            ],
            ..RenderState::default()
        });
        let marked = |x: f64| walls.strike(x - 1.0, 1.0, 0.0).expect("on a face").marked;
        assert!(marked(0.0));
        assert!(!marked(10.0), "a dragon's tooth slopes away from its box");
        assert!(!marked(20.0), "a hedgehog is open beams");
    }

    #[test]
    fn puffs_roll_round_a_wall_and_glide_along_it() {
        let mut walls = container();
        let mut puffs = PoolBuffer::new(4);
        let place = |x: f32| {
            record(
                Mat4::from_translation(Vec3::new(x, 1.0, 1.5)),
                [1.0; 4],
                [0.0; 4],
            )
        };
        puffs.push(place(0.0));
        puffs.push(place(0.01));
        walls.keep_puffs_clear(&mut puffs);
        let [a, b] = [0, 1].map(|i| puffs.records()[i].translation());
        assert!(a.z >= 1.2 + CLEARANCE - 1e-4, "{a}");
        assert!((b - a).length() < 0.02, "the push is continuous");
        assert_eq!(puffs.records()[0].tint[3], 1.0);
        let soft = walls.soft_boxes();
        assert_eq!(soft[3][3], 1.0, "the shader softens the smoke against it");
        assert_eq!(soft[0], [0.0, 0.0, 3.0, 1.2]);
    }

    /// Every puff pushed against every wall in order, as before the grid.
    fn pushed_against_every_wall(walls: &Walls, puffs: &mut PoolBuffer) -> Vec<f32> {
        let mut smoke = vec![0.0; walls.boxes.len()];
        let boxes: Vec<PuffBox> = walls
            .boxes
            .iter()
            .map(|wall| PuffBox {
                x: wall.x as f32,
                z: wall.z as f32,
                half_w: wall.half_w as f32,
                half_d: wall.half_d as f32,
                height: wall.height as f32,
            })
            .collect();
        for puff in puffs.records_mut() {
            let radius = Vec3::from_slice(&puff.world_rows[0][..3]).length();
            let weight = puff.tint[3] * radius * radius;
            let (center, pushes, _) = push_puff(
                puff.translation(),
                radius,
                weight,
                &boxes,
                None,
                &mut |i, w| {
                    smoke[i as usize] += w;
                },
            );
            if pushes > 0 {
                for (row, value) in puff.world_rows.iter_mut().zip(center.to_array()) {
                    row[3] = value;
                }
            }
            if pushes > 1 {
                puff.tint[3] *= SQUEEZED;
            }
        }
        smoke
    }

    #[test]
    fn a_wide_yards_far_wall_still_has_a_cell() {
        // Over a kilometre the cells widen; the far wall's edge is in the last one.
        let wall = |x: f64, half: f64| WallBox {
            x,
            z: 0.0,
            half_w: half,
            half_d: 1.0,
            height: 3.0,
            material: Some(WallMaterial::Stone),
            marked: true,
            color: 0x808080,
            trim: 0x808080,
            roof: 0x808080,
        };
        let mut walls = Walls {
            boxes: vec![wall(0.0, 1.0), wall(1200.0, 0.0)],
            ..Walls::default()
        };
        let mut puffs = PoolBuffer::new(1);
        puffs.push(record(
            Mat4::from_translation(Vec3::new(1200.1, 1.0, 0.0)),
            [1.0; 4],
            [0.0; 4],
        ));
        walls.keep_puffs_clear(&mut puffs);
        assert!(walls.grid.columns <= MAX_GRID_CELLS);
        assert!(
            puffs.records()[0].translation().x > 1200.1,
            "pushed off the far wall"
        );
    }

    #[test]
    fn the_wall_grid_pushes_every_puff_as_the_whole_list_would() {
        let mut random = crate::effects::random::CosmeticRandom::seeded(5);
        let mut pick = |low: f64, high: f64| low + (high - low) * random.next_f64();
        let mut walls = Walls::default();
        for _ in 0..60 {
            walls.boxes.push(WallBox {
                x: pick(-70.0, 70.0),
                z: pick(-70.0, 70.0),
                half_w: pick(0.4, 6.0),
                half_d: pick(0.4, 6.0),
                height: pick(0.8, 7.0),
                material: Some(WallMaterial::Stone),
                marked: true,
                color: 0x808080,
                trim: 0x808080,
                roof: 0x808080,
            });
        }
        // Puffs of every size, many beside or inside the boxes.
        let mut puffs = PoolBuffer::new(2000);
        for i in 0..2000 {
            let (x, z) = if i % 2 == 0 {
                let wall = walls.boxes[i % walls.boxes.len()];
                (
                    wall.x + pick(-1.4, 1.4) * wall.half_w,
                    wall.z + pick(-1.4, 1.4) * wall.half_d,
                )
            } else {
                (pick(-80.0, 80.0), pick(-80.0, 80.0))
            };
            let size = pick(0.1, 4.0) as f32;
            puffs.push(record(
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(size),
                    glam::Quat::IDENTITY,
                    Vec3::new(x as f32, pick(0.0, 8.0) as f32, z as f32),
                ),
                [1.0, 1.0, 1.0, pick(0.1, 1.0) as f32],
                [0.0; 4],
            ));
        }
        let mut expected = puffs.clone();
        let mut smoke = pushed_against_every_wall(&walls, &mut expected);
        walls.keep_puffs_clear(&mut puffs);
        assert_eq!(puffs.records(), expected.records());
        // The same smokiest walls go to the shader.
        let mut soft = [[0.0; 4]; 4];
        let mut count = 0;
        while count < SOFT_WALLS {
            let Some((index, _)) = smoke
                .iter()
                .enumerate()
                .filter(|(_, s)| **s > 0.0)
                .max_by(|a, b| a.1.total_cmp(b.1))
            else {
                break;
            };
            smoke[index] = 0.0;
            let wall = walls.boxes[index];
            soft[count] = [
                wall.x as f32,
                wall.z as f32,
                wall.half_w as f32,
                wall.half_d as f32,
            ];
            soft[3][count] = wall.height as f32;
            count += 1;
        }
        soft[3][3] = count as f32;
        assert_eq!(walls.soft_boxes(), soft);
        assert_eq!(count, SOFT_WALLS);
    }
}
