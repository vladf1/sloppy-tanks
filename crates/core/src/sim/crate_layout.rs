//! Cargo crate construction shared by the standing crate model and its physical
//! debris: the board dimensions, and the sections a broken crate falls apart into.
//! The simulation turns those sections into bodies; the model draws each from its
//! [`CratePiece`] and size, so a section looks the same wherever it lands.

use std::f64::consts::{FRAC_PI_2, PI};

use serde::{Deserialize, Serialize};

/// Height of the skids a crate stands on; its walls start here.
pub const CRATE_BASE: f64 = 0.2;
/// Thickness of the wall sheathing planks.
pub const CRATE_PLANK: f64 = 0.045;
/// Thickness of the lid boards.
pub const CRATE_LID: f64 = 0.05;
/// How far the framing battens and braces stand proud of the boards.
pub const CRATE_BATTEN: f64 = 0.045;
/// Width of a framing batten across its grain.
pub const CRATE_BATTEN_WIDTH: f64 = 0.15;
/// Width a face divides its height (or the lid its depth) into whole boards near.
pub const CRATE_BOARD_PITCH: f64 = 0.3;
/// Width of a skid across its grain.
pub const CRATE_SKID_WIDTH: f64 = 0.2;
/// Clearance between neighbouring sections so they start apart, not interpenetrating.
const SECTION_GAP: f64 = 0.02;

/// How a wall or the lid comes apart: its pieces' lengths in sixths, in order along
/// it. A few patterns rather than any cut keep section sizes repeating, so crates
/// share their sections' models.
pub const CRATE_SPLITS: [&[u8]; 5] = [&[6], &[2, 4], &[3, 3], &[4, 2], &[2, 2, 2]];
/// How likely each of [`CRATE_SPLITS`] is for the long walls, the short ends (stiffer,
/// so more often whole) and the lid.
const LONG_WALL_ODDS: [f64; 5] = [0.04, 0.18, 0.18, 0.18, 0.42];
const END_ODDS: [f64; 5] = [0.2, 0.25, 0.2, 0.25, 0.1];
const LID_ODDS: [f64; 5] = [0.04, 0.22, 0.22, 0.22, 0.3];
/// Lengths of the loose planks that can fall out, as a share of the crate's width.
pub const CRATE_PLANK_LENGTHS: [f64; 3] = [0.4, 0.55, 0.7];
const MAX_LOOSE_PLANKS: usize = 2;

/// A section of a broken crate. Wall sections are seen from outside with their
/// boards along x; lid sections lie with their boards along x, torn along their
/// y edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CratePiece {
    /// A wall that came away whole, from corner to corner.
    Wall,
    /// The left part of a wall that split (torn on its right).
    WallLeft,
    /// A middle part of a wall that split in three (torn on both sides).
    WallMiddle,
    /// The right part of a wall that split (torn on its left).
    WallRight,
    /// The lid in one piece.
    Lid,
    /// The front part of a split lid (torn along its +y edge).
    LidFront,
    /// The middle of a lid split in three (torn along both edges).
    LidMiddle,
    /// The back part of a split lid (torn along its -y edge).
    LidBack,
    Skid,
    /// A single plank torn loose, broken at both ends.
    Plank,
}

impl CratePiece {
    /// The edges (low, high along its boards for a wall; along y for the lid) where
    /// a section tore rather than met a corner.
    pub fn torn(self) -> (bool, bool) {
        match self {
            CratePiece::WallLeft | CratePiece::LidFront => (false, true),
            CratePiece::WallRight | CratePiece::LidBack => (true, false),
            CratePiece::WallMiddle | CratePiece::LidMiddle | CratePiece::Plank => (true, true),
            CratePiece::Wall | CratePiece::Lid | CratePiece::Skid => (false, false),
        }
    }

    fn of_split(lid: bool, index: usize, count: usize) -> Self {
        match (lid, count == 1, index == 0, index + 1 == count) {
            (false, true, ..) => CratePiece::Wall,
            (true, true, ..) => CratePiece::Lid,
            (false, _, true, _) => CratePiece::WallLeft,
            (true, _, true, _) => CratePiece::LidFront,
            (false, _, _, true) => CratePiece::WallRight,
            (true, _, _, true) => CratePiece::LidBack,
            (false, ..) => CratePiece::WallMiddle,
            (true, ..) => CratePiece::LidMiddle,
        }
    }
}

/// How one crate falls apart: each wall's split (front, back, right, left) and the
/// lid's, as indices into [`CRATE_SPLITS`], the loose planks (indices into
/// [`CRATE_PLANK_LENGTHS`]) and whether a skid comes away.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CrateBreak {
    pub walls: [usize; 4],
    pub lid: usize,
    pub planks: Vec<usize>,
    pub skid: bool,
}

impl CrateBreak {
    /// A random break from `next`, a stream of draws in [0, 1).
    pub fn random(mut next: impl FnMut() -> f64) -> Self {
        let mut pick = |odds: &[f64]| {
            let mut roll = next();
            for (index, &chance) in odds.iter().enumerate() {
                if roll < chance {
                    return index;
                }
                roll -= chance;
            }
            odds.len() - 1
        };
        let walls = [
            pick(&LONG_WALL_ODDS),
            pick(&LONG_WALL_ODDS),
            pick(&END_ODDS),
            pick(&END_ODDS),
        ];
        let lid = pick(&LID_ODDS);
        let planks = (0..pick(&[0.1, 0.4, 0.5]))
            .map(|_| pick(&[1.0 / 3.0; 3]))
            .collect();
        let skid = next() < 0.5;
        Self {
            walls,
            lid,
            planks,
            skid,
        }
    }
}

/// Where a section sat in the standing crate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrateSection {
    pub piece: CratePiece,
    /// Centre relative to the cover's x/z; y from the ground.
    pub center: [f64; 3],
    /// The section's extent along its boards (x), up its face (y) and through it (z).
    pub size: [f64; 3],
    /// Euler XYZ that turns the section's outer face (+z) to where it faced.
    pub rotation: [f64; 3],
    /// The outward direction of the face it belonged to.
    pub normal: [f64; 3],
}

/// Wall sections: sheathing with its battens on the outside.
pub fn crate_wall_thickness() -> f64 {
    CRATE_PLANK + CRATE_BATTEN
}

/// Lid sections: boards with the frame on top.
pub fn crate_lid_thickness() -> f64 {
    CRATE_LID + CRATE_BATTEN
}

/// The pieces `length` splits into by `split` (an index into [`CRATE_SPLITS`]):
/// each one's centre along it and its length, clear of its neighbours.
fn split_lengths(length: f64, split: usize) -> Vec<(f64, f64)> {
    let mut start = -length / 2.0;
    let parts = CRATE_SPLITS[split];
    parts
        .iter()
        .enumerate()
        .map(|(index, &sixths)| {
            let span = length * f64::from(sixths) / 6.0;
            // Half a gap either side of each cut; none at the crate's corners.
            let low = if index == 0 { 0.0 } else { SECTION_GAP / 2.0 };
            let high = if index + 1 == parts.len() {
                0.0
            } else {
                SECTION_GAP / 2.0
            };
            let kept = span - low - high;
            let center = start + low + kept / 2.0;
            start += span;
            (center, kept)
        })
        .collect()
}

/// The sections a `w` × `d` × `h` crate breaks into as `how` says. The loose planks
/// start inside the crate, where nothing else is.
pub fn crate_sections(w: f64, d: f64, h: f64, how: &CrateBreak) -> Vec<CrateSection> {
    let wall = crate_wall_thickness();
    let wall_height = h - CRATE_LID - CRATE_BASE - SECTION_GAP;
    let wall_y = CRATE_BASE + wall_height / 2.0;
    let end_length = d - 2.0 * wall - 2.0 * SECTION_GAP;
    let mut sections = Vec::with_capacity(12);
    // Front (+z), back, right (+x) and left: where the wall stands, the direction
    // its boards run seen from outside, its length and the yaw that faces it out.
    let walls = [
        ([0.0, d / 2.0], [1.0, 0.0], w, 0.0, [0.0, 0.0, 1.0]),
        ([0.0, -d / 2.0], [-1.0, 0.0], w, PI, [0.0, 0.0, -1.0]),
        (
            [w / 2.0, 0.0],
            [0.0, -1.0],
            end_length,
            FRAC_PI_2,
            [1.0, 0.0, 0.0],
        ),
        (
            [-w / 2.0, 0.0],
            [0.0, 1.0],
            end_length,
            -FRAC_PI_2,
            [-1.0, 0.0, 0.0],
        ),
    ];
    for (index, ([x, z], [ux, uz], length, yaw, normal)) in walls.into_iter().enumerate() {
        let parts = split_lengths(length, how.walls[index]);
        let count = parts.len();
        for (part, (along, span)) in parts.into_iter().enumerate() {
            sections.push(CrateSection {
                piece: CratePiece::of_split(false, part, count),
                center: [x + ux * along, wall_y, z + uz * along],
                size: [span, wall_height, wall],
                rotation: [0.0, yaw, 0.0],
                normal,
            });
        }
    }
    // The lid's y runs along -z.
    let lid = crate_lid_thickness();
    let lid_y = h - CRATE_LID + lid / 2.0;
    let parts = split_lengths(d, how.lid);
    let count = parts.len();
    for (part, (along, span)) in parts.into_iter().enumerate() {
        sections.push(CrateSection {
            piece: CratePiece::of_split(true, part, count),
            center: [0.0, lid_y, -along],
            size: [w, span, lid],
            rotation: [-FRAC_PI_2, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
        });
    }
    if how.skid {
        sections.push(CrateSection {
            piece: CratePiece::Skid,
            center: [-w * 0.34, (CRATE_BASE - SECTION_GAP) / 2.0, 0.0],
            size: [d, CRATE_BASE - SECTION_GAP, CRATE_SKID_WIDTH],
            rotation: [0.0, FRAC_PI_2, 0.0],
            normal: [0.0, -1.0, 0.0],
        });
    }
    for (index, &length) in how.planks.iter().take(MAX_LOOSE_PLANKS).enumerate() {
        let level = index as f64 - 0.5;
        sections.push(CrateSection {
            piece: CratePiece::Plank,
            center: [0.0, wall_y + level * 0.5, level * 0.6],
            size: [
                w * CRATE_PLANK_LENGTHS[length],
                CRATE_BOARD_PITCH - 0.012,
                CRATE_PLANK,
            ],
            rotation: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        });
    }
    sections
}

/// Every section and size a `w` × `d` × `h` crate can break into, for building
/// their models ahead of the first break.
pub fn crate_section_looks(w: f64, d: f64, h: f64) -> Vec<(CratePiece, [f64; 3])> {
    let mut looks: Vec<(CratePiece, [f64; 3])> = Vec::new();
    let mut every = CrateBreak {
        planks: vec![0, 1],
        skid: true,
        ..CrateBreak::default()
    };
    for split in 0..CRATE_SPLITS.len() {
        every.walls = [split; 4];
        every.lid = split;
        for plank in 0..CRATE_PLANK_LENGTHS.len() {
            every.planks = vec![plank, plank];
            for section in crate_sections(w, d, h, &every) {
                if !looks.contains(&(section.piece, section.size)) {
                    looks.push((section.piece, section.size));
                }
            }
        }
    }
    looks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each section's world-space box, assuming its rotation only swaps axes.
    fn bounds(section: &CrateSection) -> ([f64; 3], [f64; 3]) {
        let [x, y, z] = section.size;
        let sideways = section.rotation[1].abs() > 1.0 && section.rotation[1].abs() < 2.0;
        let extent = if section.rotation[0] != 0.0 {
            [x, z, y]
        } else if sideways {
            [z, y, x]
        } else {
            [x, y, z]
        };
        let min = std::array::from_fn(|i| section.center[i] - extent[i] / 2.0);
        let max = std::array::from_fn(|i| section.center[i] + extent[i] / 2.0);
        (min, max)
    }

    #[test]
    fn every_break_starts_apart_inside_the_crate() {
        let (w, d, h) = (3.1, 3.0, 2.6);
        for split in 0..CRATE_SPLITS.len() {
            for other in 0..CRATE_SPLITS.len() {
                let how = CrateBreak {
                    walls: [split, other, other, split],
                    lid: (split + other) % CRATE_SPLITS.len(),
                    planks: vec![2, 1],
                    skid: true,
                };
                let sections = crate_sections(w, d, h, &how);
                let boxes: Vec<_> = sections.iter().map(bounds).collect();
                for (i, (min, max)) in boxes.iter().enumerate() {
                    assert!(min[1] >= -1e-9 && max[1] <= h + CRATE_BATTEN + 1e-9, "{i}");
                    assert!(min[0] >= -w / 2.0 - CRATE_BATTEN - 1e-9, "{i}");
                    assert!(max[2] <= d / 2.0 + CRATE_BATTEN + 1e-9, "{i}");
                    for (j, (other_min, other_max)) in boxes.iter().enumerate().skip(i + 1) {
                        let overlap = (0..3)
                            .all(|k| min[k] + 1e-9 < other_max[k] && other_min[k] + 1e-9 < max[k]);
                        assert!(!overlap, "{how:?}: sections {i} and {j} overlap");
                    }
                }
                // The walls and lid still cover the crate's faces end to end.
                let lid: f64 = sections
                    .iter()
                    .filter(|s| s.rotation[0] != 0.0)
                    .map(|s| s.size[1])
                    .sum();
                let gaps = (CRATE_SPLITS[how.lid].len() - 1) as f64 * SECTION_GAP;
                assert!((lid + gaps - d).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn breaks_vary_and_every_section_has_a_prepared_look() {
        let mut rng = crate::sim::math::Random::new(7.0);
        let breaks: Vec<_> = (0..40).map(|_| CrateBreak::random(|| rng.next())).collect();
        let distinct = breaks
            .iter()
            .enumerate()
            .filter(|(i, how)| !breaks[..*i].contains(how))
            .count();
        assert!(distinct > 20, "breaks repeat: {distinct} of 40 differ");
        let (w, d, h) = (3.0, 3.0, 2.6);
        let looks = crate_section_looks(w, d, h);
        for how in &breaks {
            let sections = crate_sections(w, d, h, how);
            assert!((5..=18).contains(&sections.len()), "{}", sections.len());
            for section in sections {
                assert!(
                    looks.contains(&(section.piece, section.size)),
                    "{section:?}"
                );
            }
        }
    }
}
