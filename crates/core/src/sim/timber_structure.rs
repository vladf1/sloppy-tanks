//! A timber wall as a structure: its posts planted in the ground and the beams stacked
//! between them, each held in the posts' slots at both ends: the timber wall's builder
//! for the structure engine ([`super::structure`]).
//!
//! One shell knocks a beam out and leaves a gap that shells and sight pass through; the
//! beams over it stay up, held by the posts. A post shot out snaps over its stump, which
//! stands on in the ground, and drops the beams it held (all but the one lying on the
//! ground); a beam whose wall ends open against the neighbouring corner post sits in that
//! post's slot at its open end and hangs from its own post at the other.
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
use super::timber_layout::{TimberJoin, TimberPartKind, TimberWall, post_partner, timber_parts};

/// What a part of a timber wall is: its member's kind and index among the wall's beams
/// (bottom up), posts or stumps ([`timber_parts`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimberRole {
    pub kind: TimberPartKind,
    pub index: usize,
}

/// A beam breaks to a standard shell (40) that strikes it, but not to its burst behind the
/// wall; a post, like a watchtower's leg, to the second shell's burst at it.
const BEAM: Material = Material {
    strength: 34.0,
    density: TIMBER_DENSITY,
    surface: DebrisMaterial::Wood,
    breaking: Breaking::Dislodges,
    tear: 0.0,
};
const POST: Material = Material {
    strength: 60.0,
    breaking: Breaking::Splinters,
    ..BEAM
};
const TIMBER_DENSITY: f64 = 0.35;
/// The share of a post's support a beam's end in its slot takes.
const SLOT: f32 = 0.9;
/// A post rests on its stump: all of the stump's support.
const ON_STUMP: f32 = 1.0;
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
    let members = timber_parts(&TimberWall {
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
                match member.kind {
                    TimberPartKind::Beam => BEAM,
                    TimberPartKind::Post | TimberPartKind::Stump => POST,
                },
                Role::Timber(TimberRole {
                    kind: member.kind,
                    index: member.index,
                }),
            );
            let mut part = Part::new(
                vec![PartShape::aligned(center - half, center + half)],
                material,
                role,
            );
            if member.kind != TimberPartKind::Beam {
                // A stump planted in the ground holds its post upright, and the wall with it.
                part.grounded = member.kind == TimberPartKind::Stump;
                part.anchor = part.grounded;
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
    for &post in &posts {
        let stump = post_partner(&members, &members[post]).expect("a post's stump");
        structure.supports[post].push(Support {
            by: stump as u16,
            share: ON_STUMP,
            seat: 0.0,
            end: End::Any,
        });
    }
    for index in 0..count {
        let part = &structure.parts[index];
        if part.grounded || !is(part, TimberPartKind::Beam) {
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
    use crate::sim::timber_layout::BEAM_COUNT;

    fn wall() -> Arc<Structure> {
        timber_structure(3.7, 0.9, 2.8, None)
    }

    #[test]
    fn a_whole_wall_stands_and_a_beam_shot_out_leaves_the_rest_up() {
        let wall = wall();
        assert_eq!(wall.parts.len(), BEAM_COUNT + 4);
        assert!(wall.unsupported(&PartSet::EMPTY).is_empty());
        for beam in 0..BEAM_COUNT {
            let falling = wall.unsupported(&PartSet::from_indices([beam]));
            assert!(falling.is_empty(), "beam {beam} out drops {falling:?}");
        }
    }

    #[test]
    fn a_post_shot_out_drops_the_beams_it_held_but_the_one_on_the_ground() {
        let wall = wall();
        for post in [BEAM_COUNT, BEAM_COUNT + 1] {
            let falling = wall.unsupported(&PartSet::from_indices([post]));
            assert_eq!(falling, PartSet::from_indices(1..BEAM_COUNT), "post {post}");
            let falls = wall.falls(&PartSet::from_indices([post]), &PartSet::EMPTY, DVec3::ZERO);
            assert!(!falls.is_empty());
        }
    }

    #[test]
    fn a_post_snaps_over_its_stump_and_a_stump_shot_out_drops_its_post() {
        let wall = wall();
        for (post, stump) in [(BEAM_COUNT, BEAM_COUNT + 2), (BEAM_COUNT + 1, BEAM_COUNT + 3)] {
            let (post_part, stump_part) = (&wall.parts[post], &wall.parts[stump]);
            assert!(stump_part.grounded && stump_part.anchor && !post_part.grounded);
            assert!((post_part.min.y - stump_part.max.y).abs() < 1e-9, "post {post} on its stump");
            // Under a shell at hull height, which takes the post over it.
            assert!(stump_part.max.y < 0.5 && post_part.min.y < 1.0);
            assert!(!wall.unsupported(&PartSet::from_indices([post])).contains(stump));
            let falling = wall.unsupported(&PartSet::from_indices([stump]));
            assert!(falling.contains(post), "stump {stump} out drops its post");
            assert!(PartSet::from_indices(1..BEAM_COUNT).iter().all(|beam| falling.contains(beam)));
            assert!(!falling.contains(0), "the beam on the ground stays");
        }
    }

    #[test]
    fn a_wall_open_at_an_end_hangs_its_beams_from_its_one_post() {
        let join = TimberJoin {
            open_min: true,
            ..TimberJoin::default()
        };
        let wall = timber_structure(4.1, 0.9, 2.8, Some(join));
        assert_eq!(wall.parts.len(), BEAM_COUNT + 2);
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
        assert_eq!(lone.parts.len(), 2);
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
