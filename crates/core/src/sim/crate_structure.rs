//! A cargo crate as a structure: its four walls standing on its base of skids and floor
//! and the lid resting on the walls: the crate's builder for the structure engine
//! ([`super::structure`]).
//!
//! A shell that strikes a wall blows it out, and an HE round bursts in the hold behind
//! it; the lid stays on while enough wall is left under it, and a wall left with no
//! neighbour at a corner and no lid over it topples. A wall or the lid that lands hard
//! breaks along its boards into the sections the crate's model draws
//! ([`super::crate_layout::crate_sections`]).

use std::sync::Arc;

use glam::{DQuat, DVec3, EulerRot};

use super::crate_layout::{
    CRATE_BASE, CRATE_SKID_WIDTH, CrateBreak, CratePiece, CrateSection, crate_sections,
};
use super::debris_physics::DebrisMaterial;
use super::structure::{
    Brace, Breaking, Breakup, Build, DebrisRules, Levelling, Lie, Material, Part, PartSet,
    PartShape, Role, Structure, Support, Tipping,
};

/// What a part of a crate is to its model: a wall (front, back, right, left) or the lid
/// by its face, and the section of the crate it is drawn as (none for the base).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CratePart {
    pub face: Option<usize>,
    pub section: Option<CrateSection>,
}

/// The parts: the four walls, the lid and the base.
pub const CRATE_WALLS: [usize; 4] = [0, 1, 2, 3];
pub const CRATE_LID_PART: usize = 4;
pub const CRATE_BASE_PART: usize = 5;

/// A wall or the lid blows out to a standard shell (40) that strikes it; the base of
/// skids takes more.
const BOARDS: Material = Material {
    strength: 30.0,
    density: 0.35,
    surface: DebrisMaterial::Wood,
    breaking: Breaking::Splinters,
};
const SKIDS: Material = Material {
    strength: 60.0,
    ..BOARDS
};
/// What a wall and the lid break into when they land hard: the walls in two (the end
/// walls whole), the lid in three ([`super::crate_layout::CRATE_SPLITS`]).
const LANDING: CrateBreak = CrateBreak {
    walls: [1, 3, 0, 2],
    lid: 4,
    planks: Vec::new(),
    skid: false,
};
/// The skids under a crate as shares of its width, for a wide and a narrow crate
/// (`crate_model::cargo_crate`).
const WIDE_SKIDS: [f64; 3] = [-0.36, 0.0, 0.36];
const NARROW_SKIDS: [f64; 2] = [-0.34, 0.34];
/// A crate from this wide stands on three skids.
pub const WIDE_CRATE: f64 = 2.6;
/// What is left of a crate stands while its weight lies this far inside its skids per
/// metre up.
const CRATE_TIPPING: Tipping = Tipping {
    margin: 0.0,
    rise: 0.1,
    snap: None,
};
/// A crate counts as destroyed once its walls are all gone or less than this share of it
/// stands.
const LEVELLED: f64 = 0.5;
/// A crate's fallen boards land and clear as other scenery pieces do.
const CRATE_DEBRIS: DebrisRules = DebrisRules {
    landing_speed: 1.5,
    ground_only: false,
    life: 9.5,
    life_spread: 2.0,
    deadline: 18.0,
    seam: 0.01,
};
const CRATE_DUST: f64 = 2.0;

/// A section of the crate as a box shape.
fn section_shape(section: &CrateSection) -> PartShape {
    let [rx, ry, rz] = section.rotation;
    PartShape::Box {
        center: DVec3::from_array(section.center),
        half: DVec3::from_array(section.size) / 2.0,
        rotation: DQuat::from_euler(EulerRot::XYZ, rx, ry, rz),
    }
}

fn board(section: CrateSection, face: Option<usize>) -> Part {
    let mut part = Part::new(
        vec![section_shape(&section)],
        BOARDS,
        Role::Crate(CratePart {
            face,
            section: Some(section),
        }),
    );
    part.lie = Lie::Face(DVec3::from_array(section.normal));
    part
}

/// The structure of a `w` × `d` × `h` crate.
pub fn crate_structure(w: f64, d: f64, h: f64) -> Arc<Structure> {
    let whole = crate_sections(w, d, h, &CrateBreak::default());
    let mut parts: Vec<Part> = whole
        .iter()
        .enumerate()
        .map(|(face, &section)| {
            let mut wall = board(section, Some(face));
            wall.upright = face < 4;
            wall
        })
        .collect();
    debug_assert_eq!(parts.len(), 5, "four walls and the lid");
    let skids: &[f64] = if w >= WIDE_CRATE {
        &WIDE_SKIDS
    } else {
        &NARROW_SKIDS
    };
    let shapes = skids
        .iter()
        .map(|&share| {
            let x = w * share;
            PartShape::aligned(
                DVec3::new(x - CRATE_SKID_WIDTH / 2.0, 0.0, -d / 2.0),
                DVec3::new(x + CRATE_SKID_WIDTH / 2.0, CRATE_BASE, d / 2.0),
            )
        })
        .collect();
    let mut base = Part::new(
        shapes,
        SKIDS,
        Role::Crate(CratePart {
            face: None,
            section: Some(CrateSection {
                piece: CratePiece::Skid,
                center: [0.0, CRATE_BASE / 2.0, 0.0],
                size: [d, CRATE_BASE, CRATE_SKID_WIDTH],
                rotation: [0.0, std::f64::consts::FRAC_PI_2, 0.0],
                normal: [0.0, -1.0, 0.0],
            }),
        }),
    );
    base.grounded = true;
    base.contacts = base.base();
    base.lie = Lie::Face(DVec3::Y);
    parts.push(base);
    let mut structure = Structure::new(Build::Crate, 0.0, parts, CRATE_DEBRIS);
    for wall in CRATE_WALLS {
        structure.supports[wall].push(Support {
            by: CRATE_BASE_PART as u16,
            share: 1.0,
            seat: 0.0,
            end: super::structure::End::Any,
        });
        // A wall stands braced by a neighbour at a corner or the lid over it.
        let neighbours = if wall < 2 { [2, 3] } else { [0, 1] };
        structure.parts[wall].braces = vec![Brace::any(neighbours.into_iter().chain([CRATE_LID_PART]))];
        // The lid rests along the wall's top.
        let length = structure.parts[wall].max - structure.parts[wall].min;
        structure.supports[CRATE_LID_PART].push(Support {
            by: wall as u16,
            share: 1.0,
            seat: length.x.max(length.z) as f32,
            end: super::structure::End::Any,
        });
    }
    structure.join_supports(&PartSet::EMPTY);
    for wall in CRATE_WALLS {
        let neighbours = if wall < 2 { [2, 3] } else { [0, 1] };
        for other in neighbours {
            structure.join(wall, other);
        }
    }
    structure.tipping = Some(CRATE_TIPPING);
    structure.levelling = Levelling {
        keys: PartSet::from_indices(CRATE_WALLS),
        share: LEVELLED,
        announce: true,
        dust: CRATE_DUST,
    };
    for (index, part) in structure.parts.iter_mut().enumerate() {
        part.element = index as u16;
    }
    // Each wall and the lid, landing hard, breaks along its boards.
    let first = structure.parts.len() as u16;
    let mut pieces: Vec<Part> = Vec::new();
    let mut breakups = Vec::new();
    let split = crate_sections(w, d, h, &LANDING);
    for (face, section) in whole.iter().enumerate() {
        let made: Vec<CrateSection> = split
            .iter()
            .filter(|piece| piece.normal == section.normal)
            .copied()
            .collect();
        if made.len() < 2 {
            continue;
        }
        let mut indices = Vec::new();
        for piece in made {
            let mut part = board(piece, None);
            part.element = u16::MAX;
            indices.push(first + pieces.len() as u16);
            pieces.push(part);
        }
        breakups.push(Breakup {
            needs: PartSet::from_indices([face]),
            pieces: indices,
        });
    }
    structure.pieces = pieces;
    structure.breakups = breakups;
    Arc::new(structure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_crate_stands_and_one_wall_blown_out_leaves_the_lid_on() {
        let crate_ = crate_structure(3.0, 3.0, 2.6);
        assert!(crate_.unsupported(&PartSet::EMPTY).is_empty());
        for wall in CRATE_WALLS {
            assert!(crate_.unsupported(&PartSet::from_indices([wall])).is_empty(), "{wall}");
        }
    }

    #[test]
    fn the_lid_comes_down_once_too_little_wall_is_left_under_it() {
        let crate_ = crate_structure(3.0, 3.0, 2.6);
        let gone = PartSet::from_indices([0, 2, 3]);
        let falling = crate_.unsupported(&gone);
        assert!(falling.contains(CRATE_LID_PART), "{falling:?}");
        assert!(falling.contains(1), "the lone wall goes with it: {falling:?}");
    }

    #[test]
    fn a_landing_wall_breaks_along_its_boards() {
        let crate_ = crate_structure(3.0, 3.0, 2.6);
        let bodies = crate_.landing(&PartSet::from_indices([0, CRATE_LID_PART]), false);
        assert!(bodies.len() >= 4, "{bodies:?}");
        assert!(bodies.iter().all(|body| body.len() == 1));
    }
}
