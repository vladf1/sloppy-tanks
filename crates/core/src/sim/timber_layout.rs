//! Timber wall members, shared by the standing wall model and its physical debris.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::math::{js_round, to_int32};
use super::types::Cover;

pub const TIMBER_HEALTH: f64 = 80.0;
const BEAM_COUNT: usize = 4;

/// Ends use the increasing world X/Z axis, independent of the model's yaw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimberJoin {
    #[serde(default)]
    pub open_min: bool,
    #[serde(default)]
    pub open_max: bool,
    #[serde(default)]
    pub post: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimberHit {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub size: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimberFace {
    Front,
    Back,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimberMark {
    pub x: f64,
    pub y: f64,
    pub face: TimberFace,
    pub size: f64,
    pub seed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimberPartKind {
    Beam,
    Post,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimberPart {
    pub kind: TimberPartKind,
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub yaw: f64,
    pub lean: f64,
    pub color: u32,
    pub damage: u32,
    pub damage_seed: i32,
    pub marks: Vec<TimberMark>,
}

// Manual so render views can refresh a part in place without reallocating its marks.
impl Clone for TimberPart {
    fn clone(&self) -> Self {
        let mut part = TimberPart {
            kind: self.kind,
            index: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
            h: 0.0,
            d: 0.0,
            yaw: 0.0,
            lean: 0.0,
            color: 0,
            damage: 0,
            damage_seed: 0,
            marks: Vec::with_capacity(self.marks.len()),
        };
        part.clone_from(self);
        part
    }

    fn clone_from(&mut self, source: &Self) {
        self.kind = source.kind;
        self.index = source.index;
        self.x = source.x;
        self.y = source.y;
        self.z = source.z;
        self.w = source.w;
        self.h = source.h;
        self.d = source.d;
        self.yaw = source.yaw;
        self.lean = source.lean;
        self.color = source.color;
        self.damage = source.damage;
        self.damage_seed = source.damage_seed;
        self.marks.clone_from(&source.marks);
    }
}

/// The wall footprint the members are laid out in. Position only seeds cosmetic variation.
#[derive(Clone, Copy, Debug)]
pub struct TimberWall<'a> {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub color: u32,
    pub hits: &'a [TimberHit],
    pub join: Option<TimberJoin>,
}

impl<'a> TimberWall<'a> {
    pub fn of(cover: &'a Cover) -> Self {
        Self {
            x: cover.x,
            z: cover.z,
            w: cover.w,
            h: cover.h,
            d: cover.d,
            color: cover.color,
            hits: &cover.timber_hits,
            join: cover.timber_join,
        }
    }
}

pub fn timber_damage_stage(hp: f64, max_hp: f64) -> u32 {
    if hp >= max_hp {
        0
    } else if hp > max_hp * 0.5 {
        1
    } else if hp > max_hp * 0.2 {
        2
    } else {
        3
    }
}

/// The same members are used by the standing wall and its physical debris.
pub fn timber_parts(wall: &TimberWall, stage: u32) -> Vec<TimberPart> {
    let along = wall.w > wall.d;
    let length = wall.w.max(wall.d);
    let depth = wall.w.min(wall.d);
    let yaw = if along { 0.0 } else { PI / 2.0 };
    let post_width = 0.32f64.min(length * 0.12);
    // Separate physical members must start clear of the posts, even in the loosened pose.
    let end_clearance = 0.04 + wall.h * 0.02;
    let join = wall.join.unwrap_or_default();
    let post = join.post;
    let open_negative = if along { join.open_min } else { join.open_max };
    let open_positive = if along { join.open_max } else { join.open_min };
    let inset = 0.18 + post_width / 2.0 + end_clearance;
    let beam_min = -length / 2.0 + if open_negative { 0.025 } else { inset };
    let beam_max = length / 2.0 - if open_positive { 0.025 } else { inset };
    let beam_center = (beam_min + beam_max) / 2.0;
    let pitch = (wall.h - 0.12) / BEAM_COUNT as f64;
    let colors = [wall.color, 0x94613e, 0xa66f46];
    // Cosmetic randomness is stable per wall and never consumes the combat RNG.
    // `Math.imul` converts with ToInt32, which wraps rather than saturates.
    let seed = to_int32(js_round(wall.x * 100.0)).wrapping_mul(73_856_093)
        ^ to_int32(js_round(wall.z * 100.0)).wrapping_mul(19_349_663);
    let mut parts = Vec::new();
    let beams = if post { 0 } else { BEAM_COUNT };
    for index in 0..beams {
        let damage = if index == 1 {
            stage
        } else {
            stage.saturating_sub(if index == 2 { 1 } else { 2 })
        };
        parts.push(TimberPart {
            kind: TimberPartKind::Beam,
            index,
            x: if along { beam_center } else { 0.0 },
            y: 0.06 + pitch * (index as f64 + 0.5),
            z: if along { 0.0 } else { -beam_center },
            w: beam_max - beam_min,
            h: pitch - 0.025,
            d: 0.66f64.min(depth * 0.75),
            yaw,
            lean: if index == BEAM_COUNT - 1 {
                stage as f64 * 0.006
            } else {
                0.0
            },
            color: colors[index % colors.len()],
            damage,
            marks: Vec::new(),
            damage_seed: seed ^ (index as i32 + 1).wrapping_mul(83_492_791),
        });
    }
    let sides: &[i32] = if post { &[0] } else { &[-1, 1] };
    for &side in sides {
        if (side < 0 && open_negative) || (side > 0 && open_positive) {
            continue;
        }
        let offset = side as f64 * (length / 2.0 - 0.18);
        parts.push(TimberPart {
            kind: TimberPartKind::Post,
            index: match side {
                0 => 0,
                s if s < 0 => BEAM_COUNT,
                _ => BEAM_COUNT + 1,
            },
            x: if along { offset } else { 0.0 },
            y: wall.h / 2.0,
            z: if along { 0.0 } else { -offset },
            w: if post { length } else { post_width },
            h: wall.h,
            d: depth,
            yaw,
            lean: if stage == 3 { side as f64 * 0.028 } else { 0.0 },
            color: 0x805336,
            damage: stage,
            marks: Vec::new(),
            damage_seed: seed ^ (side + 7).wrapping_mul(83_492_791),
        });
    }
    // Attach each hit to its nearest actual beam/post and only the struck face.
    for (hit_index, hit) in wall.hits.iter().enumerate() {
        let local = |part: &TimberPart| {
            let dx = hit.x - part.x;
            let dz = hit.z - part.z;
            let x = dx * yaw.cos() - dz * yaw.sin();
            let z = dx * yaw.sin() + dz * yaw.cos();
            let y = hit.y - part.y;
            let distance = (x.abs() - part.w / 2.0).max(0.0).powi(2)
                + (y.abs() - part.h / 2.0).max(0.0).powi(2)
                + (z.abs() - part.d / 2.0).max(0.0).powi(2);
            (x, y, z, distance)
        };
        // The first of equally near members wins, like the stable sort it replaces.
        let Some((nearest, (x, y, z, _))) = parts
            .iter()
            .enumerate()
            .map(|(i, part)| (i, local(part)))
            .min_by(|a, b| {
                a.1.3
                    .partial_cmp(&b.1.3)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            continue;
        };
        let part = &mut parts[nearest];
        let end = x.abs() > part.w / 2.0 - 0.015;
        part.marks.push(TimberMark {
            x: if end { z } else { x },
            y,
            face: if end {
                if x < 0.0 {
                    TimberFace::Left
                } else {
                    TimberFace::Right
                }
            } else if z < 0.0 {
                TimberFace::Back
            } else {
                TimberFace::Front
            },
            size: hit.size,
            seed: part.damage_seed ^ (hit_index as i32 + 1).wrapping_mul(0x45d9f3b),
        });
    }
    // Marks attach in the original piece coordinates, then move with a loosened beam.
    let shift = if stage >= 2 && !open_negative && !open_positive {
        0.02 * (stage as f64 - 1.0)
    } else {
        0.0
    };
    if !post {
        let beam = &mut parts[BEAM_COUNT - 1];
        beam.x += if along { shift } else { 0.0 };
        beam.z -= if along { 0.0 } else { shift };
    }
    parts
}
