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

/// A section of a broken crate. Walls and ends are seen from outside with their
/// boards along x; a lid half lies with its boards along x and its break along one
/// long edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CratePiece {
    /// The part of a long wall left of where it split (its break on the right).
    WallLeft,
    /// The part of a long wall right of where it split (its break on the left).
    WallRight,
    /// A short wall, torn along one end.
    End,
    /// The lid half whose break runs along its +y edge.
    LidFront,
    /// The lid half whose break runs along its -y edge.
    LidBack,
    Skid,
    /// A single plank torn loose, broken at both ends.
    Plank,
}

impl CratePiece {
    pub const ALL: [CratePiece; 7] = [
        CratePiece::WallLeft,
        CratePiece::WallRight,
        CratePiece::End,
        CratePiece::LidFront,
        CratePiece::LidBack,
        CratePiece::Skid,
        CratePiece::Plank,
    ];
}

/// Where a section sat in the standing crate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrateSection {
    pub piece: CratePiece,
    /// Centre relative to the cover's x/z; y from the ground.
    pub center: [f64; 3],
    /// The section's extent along its boards (x), up its face (y) and through it (z).
    pub size: [f64; 3],
    /// Yaw then pitch (Euler XYZ) that turns the section's outer face (+z) to where
    /// it faced.
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

/// The sections a `w` × `d` × `h` crate breaks into. Each long wall splits where
/// `splits[0]` and `splits[1]` (0..1 along it) say, and the lid across its depth at
/// `splits[2]`. The loose plank starts inside the crate, where nothing else is.
pub fn crate_sections(w: f64, d: f64, h: f64, splits: [f64; 3]) -> Vec<CrateSection> {
    let wall = crate_wall_thickness();
    let wall_height = h - CRATE_LID - CRATE_BASE - SECTION_GAP;
    let wall_y = CRATE_BASE + wall_height / 2.0;
    let mut sections = Vec::with_capacity(10);
    // Long walls: front (+z) and back (-z), boards along x. The back's outside runs
    // the other way, so its left section is at +x.
    for (index, (side, yaw)) in [(1.0, 0.0), (-1.0, PI)].into_iter().enumerate() {
        let split = w * (splits[index].clamp(0.3, 0.7) - 0.5);
        let left = split - SECTION_GAP / 2.0 + w / 2.0;
        let right = w / 2.0 - split - SECTION_GAP / 2.0;
        for (piece, length, along) in [
            (CratePiece::WallLeft, left, -w / 2.0 + left / 2.0),
            (CratePiece::WallRight, right, w / 2.0 - right / 2.0),
        ] {
            sections.push(CrateSection {
                piece,
                // Along the outside of the back, x runs backwards.
                center: [side * along, wall_y, side * d / 2.0],
                size: [length, wall_height, wall],
                rotation: [0.0, yaw, 0.0],
                normal: [0.0, 0.0, side],
            });
        }
    }
    // Short ends: right (+x) and left (-x), clear of the long walls' corners.
    let end_length = d - 2.0 * wall - 2.0 * SECTION_GAP;
    for side in [1.0, -1.0] {
        sections.push(CrateSection {
            piece: CratePiece::End,
            center: [side * w / 2.0, wall_y, 0.0],
            size: [end_length, wall_height, wall],
            rotation: [0.0, side * FRAC_PI_2, 0.0],
            normal: [side, 0.0, 0.0],
        });
    }
    // Lid halves across the depth; the lid's y runs along -z.
    let lid = crate_lid_thickness();
    let lid_y = h - CRATE_LID + lid / 2.0;
    let split = d * (splits[2].clamp(0.35, 0.65) - 0.5);
    let front = d / 2.0 - split - SECTION_GAP / 2.0;
    let back = d / 2.0 + split - SECTION_GAP / 2.0;
    for (piece, depth, z) in [
        (CratePiece::LidFront, front, d / 2.0 - front / 2.0),
        (CratePiece::LidBack, back, -d / 2.0 + back / 2.0),
    ] {
        sections.push(CrateSection {
            piece,
            center: [0.0, lid_y, z],
            size: [w, depth, lid],
            rotation: [-FRAC_PI_2, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
        });
    }
    sections.push(CrateSection {
        piece: CratePiece::Skid,
        center: [-w * 0.34, (CRATE_BASE - SECTION_GAP) / 2.0, 0.0],
        size: [d, CRATE_BASE - SECTION_GAP, CRATE_SKID_WIDTH],
        rotation: [0.0, FRAC_PI_2, 0.0],
        normal: [0.0, -1.0, 0.0],
    });
    sections.push(CrateSection {
        piece: CratePiece::Plank,
        center: [0.0, wall_y, 0.0],
        size: [w * 0.55, CRATE_BOARD_PITCH - 0.012, CRATE_PLANK],
        rotation: [0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
    });
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each section's world-space box, assuming its rotation only swaps axes.
    fn bounds(section: &CrateSection) -> ([f64; 3], [f64; 3]) {
        let [x, y, z] = section.size;
        let extent = match section.piece {
            CratePiece::End | CratePiece::Skid => [z, y, x],
            CratePiece::LidFront | CratePiece::LidBack => [x, z, y],
            _ => [x, y, z],
        };
        let min = std::array::from_fn(|i| section.center[i] - extent[i] / 2.0);
        let max = std::array::from_fn(|i| section.center[i] + extent[i] / 2.0);
        (min, max)
    }

    #[test]
    fn sections_start_apart_inside_the_crate() {
        let (w, d, h) = (3.1, 3.0, 2.6);
        let sections = crate_sections(w, d, h, [0.42, 0.61, 0.5]);
        assert_eq!(sections.len(), 10);
        let boxes: Vec<_> = sections.iter().map(bounds).collect();
        for (i, (min, max)) in boxes.iter().enumerate() {
            // Within the crate's outline, battens included.
            assert!(min[1] >= -1e-9 && max[1] <= h + CRATE_BATTEN + 1e-9, "{i}");
            assert!(min[0] >= -w / 2.0 - CRATE_BATTEN - 1e-9, "{i}");
            assert!(max[2] <= d / 2.0 + CRATE_BATTEN + 1e-9, "{i}");
            for (j, (other_min, other_max)) in boxes.iter().enumerate().skip(i + 1) {
                let overlap = (0..3).all(|k| min[k] < other_max[k] && other_min[k] < max[k]);
                assert!(!overlap, "sections {i} and {j} overlap");
            }
        }
    }
}
