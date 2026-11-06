//! A timber wall as a structure: its posts planted in the ground and the beams stacked
//! between them, each held in the posts' slots at both ends: the timber wall's builder
//! for the structure engine ([`super::structure`]).
//!
//! One shell knocks a beam out and leaves a gap that shells and sight pass through; the
//! beams over it stay up, held by the posts. A post shot out drops the beams it held
//! (all but the one lying on the ground); a beam whose wall ends open against the
//! neighbouring corner post sits in that post's slot at its open end and hangs from its
//! own post at the other.
//!
//! Members are laid out along the build frame's x ([`timber_parts`]); a wall running
//! along z turns its build frame a quarter turn.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::DVec3;

use super::debris_physics::DebrisMaterial;
use super::structure::{
    Breaking, Build, DebrisRules, End, Levelling, Lie, Material, Part, PartSet, PartShape, Role,
    Structure, Support, Tipping,
};
use super::timber_layout::{TimberJoin, TimberPartKind, TimberWall, timber_members};

/// What a part of a timber wall is: its member's kind and index among the wall's beams
/// (bottom up) or posts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimberRole {
    pub kind: TimberPartKind,
    pub index: usize,
    /// Which length of its beam, of how many.
    pub segment: Option<(usize, usize)>,
}

/// A beam breaks to a standard shell (40) that strikes it, but not to its burst behind the
/// wall; a post, like a watchtower's leg, to the second shell's burst at it.
const BEAM: Material = Material {
    strength: 34.0,
    density: TIMBER_DENSITY,
    surface: DebrisMaterial::Wood,
    breaking: Breaking::Splinters,
    tear: 0.0,
};
const POST: Material = Material {
    strength: 60.0,
    ..BEAM
};
const TIMBER_DENSITY: f64 = 0.35;
/// The share of a post's support a beam's end in its slot takes.
const SLOT: f32 = 0.9;
/// What is left of a wall stands while its weight lies this far inside its footing per
/// metre up; a post planted in the ground stands on its own.
const WALL_TIPPING: Tipping = Tipping {
    margin: 0.0,
    rise: 0.1,
    snap: None,
};
/// A wall counts as destroyed once its beams are all gone, or less than this share of it
/// stands.
const LEVELLED: f64 = 0.34;
/// A wall's fallen members land and clear as other scenery pieces do. Standing members'
/// colliders reach this far past their faces, a beam's end and its post together over the
/// clearance between them, so no shell slips between them.
const WALL_DEBRIS: DebrisRules = DebrisRules {
    landing_speed: 1.5,
    firm_only: false,
    one_clock: false,
    life: 9.5,
    life_spread: 2.0,
    deadline: 18.0,
    seam: 0.05,
};
/// The dust a destroyed wall raises (m).
const WALL_DUST: f64 = 2.0;
/// A beam this low stands on the ground.
const GROUND_REACH: f64 = 0.1;

/// The structure of a `w` × `d` × `h` timber wall whose ends meet its neighbours as
/// `join` says.
pub fn timber_structure(w: f64, d: f64, h: f64, join: Option<TimberJoin>) -> Arc<Structure> {
    let along = w > d;
    let (length, depth) = (w.max(d), w.min(d));
    // Laid out along x; the world's z-running wall is that turned a quarter turn.
    let join = join.map(|join| {
        if along {
            join
        } else {
            TimberJoin {
                open_min: join.open_max,
                open_max: join.open_min,
                ..join
            }
        }
    });
    let members = timber_members(&TimberWall {
        x: 0.0,
        z: 0.0,
        w: length,
        h,
        d: depth,
        color: 0,
        join,
    });
    let parts: Vec<Part> = members
        .iter()
        .map(|member| {
            let half = DVec3::new(member.w, member.h, member.d) / 2.0;
            let center = DVec3::new(member.x, member.y, member.z);
            let (material, role) = (
                if member.kind == TimberPartKind::Post {
                    POST
                } else {
                    BEAM
                },
                Role::Timber(TimberRole {
                    kind: member.kind,
                    index: member.index,
                    segment: member.segment,
                }),
            );
            let mut part = Part::new(
                vec![PartShape::aligned(center - half, center + half)],
                material,
                role,
            );
            if member.kind == TimberPartKind::Post {
                part.grounded = true;
                part.anchor = true;
                part.upright = true;
                part.lie = Lie::Length(DVec3::Y);
                part.paint = Some(member.color);
            } else {
                part.grounded = part.min.y <= GROUND_REACH;
                part.lie = Lie::Length(DVec3::X);
                // The wall's own colour is its first beam's.
                part.paint = (member.index > 0).then_some(member.color);
            }
            if part.grounded {
                part.contacts = part.base();
            }
            part
        })
        .collect();
    let count = parts.len();
    let mut structure = Structure::new(Build::Timber, if along { 0.0 } else { FRAC_PI_2 }, parts, WALL_DEBRIS);
    let is = |part: &Part, kind: TimberPartKind| {
        matches!(part.role, Role::Timber(role) if role.kind == kind)
    };
    let posts: Vec<usize> = (0..count)
        .filter(|&index| is(&structure.parts[index], TimberPartKind::Post))
        .collect();
    let segment = |part: &Part| match part.role {
        Role::Timber(TimberRole {
            segment: Some(segment),
            ..
        }) => Some(segment),
        _ => None,
    };
    for index in 0..count {
        let part = &structure.parts[index];
        if part.grounded && segment(part).is_none() {
            continue;
        }
        // A length of a beam between two posts: its end lengths sit in their posts'
        // slots, standing as stubs alone; a middle one bears on the lengths either side.
        if let Some((k, of)) = segment(part) {
            let supports = if k == 0 || k + 1 == of {
                let middle = part.middle().x;
                let post = posts
                    .iter()
                    .copied()
                    .min_by(|&a, &b| {
                        let gap = |post: usize| (structure.parts[post].middle().x - middle).abs();
                        gap(a).total_cmp(&gap(b))
                    })
                    .expect("a segmented beam has its posts");
                vec![(post, End::Any, SLOT)]
            } else {
                vec![(index - 1, End::A, 1.0), (index + 1, End::B, 1.0)]
            };
            for (by, end, share) in supports {
                structure.supports[index].push(Support {
                    by: by as u16,
                    share,
                    seat: 0.0,
                    end,
                });
            }
            continue;
        }
        // A beam sits in a slot of each post it reaches; with one end open it sits in
        // the neighbouring corner post's at that end, and hangs from its own.
        let middle = part.middle().x;
        let ends: Vec<(usize, End)> = posts
            .iter()
            .map(|&post| {
                let end = if structure.parts[post].middle().x < middle {
                    End::A
                } else {
                    End::B
                };
                (post, end)
            })
            .collect();
        let both = ends.iter().any(|(_, end)| *end == End::A) && ends.iter().any(|(_, end)| *end == End::B);
        for (post, end) in ends {
            structure.supports[index].push(Support {
                by: post as u16,
                share: SLOT,
                seat: 0.0,
                end: if both { end } else { End::Any },
            });
        }
    }
    structure.join_supports(&PartSet::EMPTY);
    // A beam's lengths are one timber: what comes down of it together falls as one.
    for index in 1..count {
        let (a, b) = (segment(&structure.parts[index - 1]), segment(&structure.parts[index]));
        if let (Some((ka, _)), Some((kb, _))) = (a, b)
            && kb == ka + 1
        {
            structure.join(index - 1, index);
        }
    }
    // Beams lying on each other are fastened by their posts, not to each other: a body
    // of them comes down together only through a post.
    structure.whole = true;
    structure.tipping = Some(WALL_TIPPING);
    let beams = PartSet::from_indices(
        (0..count).filter(|&index| is(&structure.parts[index], TimberPartKind::Beam)),
    );
    structure.levelling = Levelling {
        keys: if beams.is_empty() { structure.all() } else { beams },
        share: LEVELLED,
        announce: true,
        dust: WALL_DUST,
    };
    for (index, part) in structure.parts.iter_mut().enumerate() {
        part.element = index as u16;
    }
    Arc::new(structure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::timber_layout::{BEAM_COUNT, BEAM_SEGMENTS};

    fn wall() -> Arc<Structure> {
        timber_structure(3.7, 0.9, 2.8, None)
    }

    #[test]
    fn a_whole_wall_stands_and_a_beam_shot_out_leaves_the_rest_up() {
        let wall = wall();
        assert_eq!(wall.parts.len(), BEAM_COUNT * BEAM_SEGMENTS + 2);
        assert!(wall.unsupported(&PartSet::EMPTY).is_empty());
        // Any length of a beam shot out: the stubs in the posts stand, and so does
        // everything else; an end length out drops the lengths it held up.
        for beam in 0..BEAM_COUNT {
            let [first, middle, last] = [0, 1, 2].map(|k| beam * BEAM_SEGMENTS + k);
            let falling = wall.unsupported(&PartSet::from_indices([middle]));
            assert!(falling.is_empty(), "beam {beam}'s middle out drops {falling:?}");
            let falling = wall.unsupported(&PartSet::from_indices([first]));
            let held = if beam == 0 { PartSet::EMPTY } else { PartSet::from_indices([middle]) };
            assert_eq!(falling, held, "beam {beam}'s end out");
            assert!(!falling.contains(last));
        }
    }

    #[test]
    fn a_post_shot_out_drops_the_beam_lengths_it_held_but_the_ones_on_the_ground() {
        let wall = wall();
        let posts = [BEAM_COUNT * BEAM_SEGMENTS, BEAM_COUNT * BEAM_SEGMENTS + 1];
        for (side, post) in posts.into_iter().enumerate() {
            let falling = wall.unsupported(&PartSet::from_indices([post]));
            let end = if side == 0 { 0 } else { BEAM_SEGMENTS - 1 };
            let expected = PartSet::from_indices((1..BEAM_COUNT).flat_map(|beam| {
                [beam * BEAM_SEGMENTS + end, beam * BEAM_SEGMENTS + 1]
            }));
            assert_eq!(falling, expected, "post {post}");
            let falls = wall.falls(&PartSet::from_indices([post]), &PartSet::EMPTY, DVec3::ZERO);
            assert!(!falls.is_empty());
        }
    }

    #[test]
    fn a_wall_open_at_an_end_hangs_its_beams_from_its_one_post() {
        let join = TimberJoin {
            open_min: true,
            ..TimberJoin::default()
        };
        let wall = timber_structure(4.1, 0.9, 2.8, Some(join));
        assert_eq!(wall.parts.len(), BEAM_COUNT + 1);
        assert!(wall.unsupported(&PartSet::EMPTY).is_empty());
        let post = BEAM_COUNT;
        assert_eq!(
            wall.unsupported(&PartSet::from_indices([post])),
            PartSet::from_indices(1..BEAM_COUNT)
        );
        let lone = timber_structure(
            0.9,
            0.9,
            2.8,
            Some(TimberJoin {
                post: true,
                ..TimberJoin::default()
            }),
        );
        assert_eq!(lone.parts.len(), 1);
        assert!(lone.unsupported(&PartSet::EMPTY).is_empty());
    }

    #[test]
    fn a_wall_along_z_is_the_same_wall_turned() {
        let x = timber_structure(3.7, 0.9, 2.8, None);
        let z = timber_structure(0.9, 3.7, 2.8, None);
        for index in 0..x.parts.len() {
            let (a, b) = (x.world_box(index), z.world_box(index));
            // The wall's x becomes the turned wall's -z.
            assert!((a.0.x + b.1.z).abs() < 1e-9 && (a.1.x + b.0.z).abs() < 1e-9, "{index}");
            assert!((a.0.z - b.0.x).abs() < 1e-9 && (a.1.y - b.1.y).abs() < 1e-9, "{index}");
        }
    }
}
