//! Timber wall members, shared by the standing wall model, its structure
//! (`timber_structure`) and its fallen members.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::math::{js_round, to_int32};

/// What a timber wall counts for in its cover's health, for the bots that weigh cover.
pub const TIMBER_HEALTH: f64 = 80.0;
/// The beams stacked between a wall's posts.
pub const BEAM_COUNT: usize = 4;
/// The lengths a beam between two posts breaks in: a shell takes the one it strikes,
/// and the ones left stand as stubs in their posts or come down without their end.
pub const BEAM_SEGMENTS: usize = 3;

/// Ends use the increasing world X/Z axis, independent of the model's yaw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TimberJoin {
    pub open_min: bool,
    pub open_max: bool,
    pub post: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimberPartKind {
    Beam,
    Post,
}

/// One member of a timber wall: a beam or a post, centred at `x, y, z` relative to the
/// wall's ground centre, `w` along its grain (`h` up, `d` through), turned `yaw` about
/// y. `seed` varies its grain and weathering.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
    pub color: u32,
    pub seed: i32,
    /// A length of a beam as the wall's structure breaks it: which of how many.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<(usize, usize)>,
}

/// The wall footprint the members are laid out in. Position only seeds cosmetic variation.
#[derive(Clone, Copy, Debug)]
pub struct TimberWall {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub color: u32,
    pub join: Option<TimberJoin>,
}

/// The members of a wall, beams first (bottom up) then posts, as the structure numbers
/// its parts.
pub fn timber_parts(wall: &TimberWall) -> Vec<TimberPart> {
    let along = wall.w > wall.d;
    let length = wall.w.max(wall.d);
    let depth = wall.w.min(wall.d);
    let yaw = if along { 0.0 } else { PI / 2.0 };
    let post_width = 0.32f64.min(length * 0.12);
    // Separate physical members must start clear of the posts.
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
            color: colors[index % colors.len()],
            seed: seed ^ (index as i32 + 1).wrapping_mul(83_492_791),
            segment: None,
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
            color: 0x805336,
            seed: seed ^ (side + 7).wrapping_mul(83_492_791),
            segment: None,
        });
    }
    parts
}

/// The wall's members as its structure numbers its parts: each beam held by a post at
/// both ends in [`BEAM_SEGMENTS`] lengths (bottom beam first, its lengths along the
/// wall's increasing axis), then the posts; a beam on one post stays whole.
pub fn timber_members(wall: &TimberWall) -> Vec<TimberPart> {
    let parts = timber_parts(wall);
    let posts = parts
        .iter()
        .filter(|part| part.kind == TimberPartKind::Post)
        .count();
    let segments = if posts == 2 { BEAM_SEGMENTS } else { 1 };
    let mut members = Vec::with_capacity(parts.len() * segments);
    for part in &parts {
        if part.kind == TimberPartKind::Post || segments == 1 {
            members.push(*part);
            continue;
        }
        // Along x for a wall along x; a z-running wall's members turned, their x on z.
        let (start, axis) = if part.yaw == 0.0 {
            (part.x - part.w / 2.0, DVecAxis::X)
        } else {
            (-part.z - part.w / 2.0, DVecAxis::NegZ)
        };
        let length = part.w / segments as f64;
        for k in 0..segments {
            let middle = start + length * (k as f64 + 0.5);
            let (x, z) = match axis {
                DVecAxis::X => (middle, part.z),
                DVecAxis::NegZ => (part.x, -middle),
            };
            members.push(TimberPart {
                x,
                z,
                w: length,
                seed: part.seed ^ (k as i32 + 3).wrapping_mul(19_349_663),
                segment: Some((k, segments)),
                ..*part
            });
        }
    }
    members
}

#[derive(Clone, Copy)]
enum DVecAxis {
    X,
    NegZ,
}
