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
}

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
    /// Its paint or stone, sRGB hex.
    color: u32,
}

/// Where a shell struck a wall: the point on its face, the face's outward normal
/// and how much room the face leaves round the point for a mark (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub material: WallMaterial,
    pub point: Vec3,
    pub normal: Vec3,
    pub room: f32,
    /// The wall's paint or stone, sRGB hex.
    pub color: u32,
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
}

impl Walls {
    pub fn sync(&mut self, state: &RenderState) {
        self.boxes.clear();
        for cover in state.covers.iter().filter(|cover| cover.alive) {
            let material = match cover.kind {
                CoverKind::Container | CoverKind::Hedgehog => Some(WallMaterial::Steel),
                CoverKind::Rock
                | CoverKind::Concrete
                | CoverKind::Teeth
                | CoverKind::House
                | CoverKind::Rubble => Some(WallMaterial::Stone),
                CoverKind::Cargo | CoverKind::Timber => None,
                _ => continue,
            };
            self.boxes.push(WallBox {
                x: cover.x,
                z: cover.z,
                half_w: cover.w * 0.5,
                half_d: cover.d * 0.5,
                height: cover.h,
                material,
                color: cover.color,
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
                    color: wall.color,
                },
            ));
        }
        best.map(|(_, strike)| strike)
    }

    /// Push every smoke puff in `puffs` out of the walls' boxes, so a billow rolls
    /// round a container or a butte instead of being sliced by it, and thin the
    /// ones wedged where no push frees them. Each puff's new place depends only on
    /// where it would have been, so a drifting billow glides along a wall.
    pub fn keep_puffs_clear(&mut self, puffs: &mut PoolBuffer) {
        self.soft = [[0.0; 4]; 4];
        if self.boxes.is_empty() {
            return;
        }
        self.smoke.clear();
        self.smoke.resize(self.boxes.len(), 0.0);
        for puff in puffs.records_mut() {
            let start = puff.translation();
            let mut center = start;
            let radius = Vec3::from_slice(&puff.world_rows[0][..3]).length();
            let clearance = radius * CLEARANCE;
            let mut pushes = 0;
            let weight = puff.tint[3] * radius * radius;
            for (wall, smoke) in self.boxes.iter().zip(&mut self.smoke) {
                let (hw, hd, h) = (wall.half_w as f32, wall.half_d as f32, wall.height as f32);
                let local = Vec3::new(center.x - wall.x as f32, center.y, center.z - wall.z as f32);
                let reach = radius * SOFT_REACH;
                if local.x.abs() >= hw + reach
                    || local.z.abs() >= hd + reach
                    || local.y >= h + reach
                {
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
                    *smoke += weight;
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
                color: 0x2f8a6a,
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
}
