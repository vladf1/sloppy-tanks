//! Timber wall members, shared by the standing wall model, its structure
//! (`timber_structure`) and its fallen members.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::math::{js_round, to_int32};

/// What a timber wall counts for in its cover's health, for the bots that weigh cover.
pub const TIMBER_HEALTH: f64 = 80.0;
/// The beams stacked between a wall's posts.
pub const BEAM_COUNT: usize = 4;
/// A post snaps over its stump, planted in the ground this high (m): under the post's
/// lower iron band and the top of the bottom beam, so a shell at hull height takes the
/// post and leaves the stump, and low enough that the post's foot counts as on the
/// ground (a post broken along with its stump shatters as the whole post did).
pub const STUMP_HEIGHT: f64 = 0.28;

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
    /// A post over its stump.
    Post,
    /// The foot of a post, planted in the ground.
    Stump,
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

/// The members of a wall, beams first (bottom up), then posts, then the posts' stumps in
/// the same order, as the structure numbers its parts. Beams are numbered bottom up,
/// posts by side (a lone post 0, the -x post [`BEAM_COUNT`], the +x one after it) and
/// stumps by their place in the list.
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
        });
    }
    let sides: &[i32] = if post { &[0] } else { &[-1, 1] };
    let stump = STUMP_HEIGHT.min(wall.h / 4.0);
    let mut stumps = Vec::new();
    for &side in sides {
        if (side < 0 && open_negative) || (side > 0 && open_positive) {
            continue;
        }
        let offset = side as f64 * (length / 2.0 - 0.18);
        let member = TimberPart {
            kind: TimberPartKind::Post,
            index: match side {
                0 => 0,
                s if s < 0 => BEAM_COUNT,
                _ => BEAM_COUNT + 1,
            },
            x: if along { offset } else { 0.0 },
            y: (stump + wall.h) / 2.0,
            z: if along { 0.0 } else { -offset },
            w: if post { length } else { post_width },
            h: wall.h - stump,
            d: depth,
            yaw,
            color: 0x805336,
            seed: seed ^ (side + 7).wrapping_mul(83_492_791),
        };
        stumps.push(TimberPart {
            kind: TimberPartKind::Stump,
            y: stump / 2.0,
            h: stump,
            seed: seed ^ (side + 13).wrapping_mul(83_492_791),
            ..member
        });
        parts.push(member);
    }
    for mut member in stumps {
        member.index = parts.len();
        parts.push(member);
    }
    parts
}

/// The whole post a standing post over its standing stump draws as: the member from the
/// ground to its top, as the wall was built.
pub fn whole_post(post: &TimberPart) -> TimberPart {
    let top = post.y + post.h / 2.0;
    TimberPart {
        y: top / 2.0,
        h: top,
        ..*post
    }
}

/// Where among a wall's `members` the stump under a post is, or the post over a stump.
pub fn post_partner(members: &[TimberPart], member: &TimberPart) -> Option<usize> {
    let kind = match member.kind {
        TimberPartKind::Post => TimberPartKind::Stump,
        TimberPartKind::Stump => TimberPartKind::Post,
        TimberPartKind::Beam => return None,
    };
    members
        .iter()
        .position(|other| other.kind == kind && other.x == member.x && other.z == member.z)
}
