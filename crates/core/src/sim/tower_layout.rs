//! Watchtower dimensions shared by the intact model, its collapse pieces, the
//! surviving foundations and the collision footprint.
//!
//! The tower is assembled from the pieces it breaks into ([`TowerPiece`]): each
//! piece's model is built in tower coordinates and its box is the collider the
//! piece falls with, so the debris that replaces the tower starts exactly where
//! the parts stood. Colliders leave small gaps so no two pieces start
//! interpenetrating.

use serde::{Deserialize, Serialize};

use super::types::FragmentShape;

pub struct TowerBase {
    pub offset: f64,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub rubble_height: f64,
    pub post_z: f64,
}

pub const TOWER_BASE: TowerBase = TowerBase {
    offset: 2.55,
    width: 1.3,
    depth: 3.0,
    height: 0.85,
    rubble_height: 1.25,
    post_z: 1.05,
};

/// Post section and the top of the posts under the main beams.
pub const POST: f64 = 0.35;
pub const POST_TOP: f64 = 4.645;
/// Height where a post snaps, just inside the top of the steel shoe that stays
/// on the footing.
pub const POST_BREAK: f64 = 1.3;
/// Main beams, joists and planks under the deck top.
pub const BEAM: f64 = 0.3;
pub const JOIST_DEPTH: f64 = 0.18;
pub const PLANK: f64 = 0.05;
pub const DECK_TOP: f64 = POST_TOP + BEAM + JOIST_DEPTH + PLANK;
/// Half extents of the deck.
pub const DECK_X: f64 = 3.0;
pub const DECK_Z: f64 = 2.5;
/// Lookout cabin: half extents, wall top (the flat soffit) and wall thickness
/// from the clapboard face to the interior lining.
pub const CABIN_X: f64 = 2.15;
pub const CABIN_Z: f64 = 1.75;
pub const CABIN_TOP: f64 = 7.3;
pub const CABIN_WALL: f64 = 0.12;
/// Window band of the cabin walls and how far the awning shutters reach out.
pub const WINDOW_SILL: f64 = 6.05;
pub const WINDOW_HEIGHT: f64 = 0.85;
const AWNING_REACH: f64 = 0.86;
const AWNING_BOTTOM: f64 = 6.65;
const AWNING_TOP: f64 = 7.12;
/// Hip roof: overhang past the cabin walls, fascia depth and pitch.
pub const ROOF_OVERHANG: f64 = 0.5;
pub const ROOF_EDGE: f64 = 0.15;
pub const ROOF_PITCH: f64 = 0.55;
/// Walkway railing: height above the deck and the inset from the deck edge.
pub const RAIL_HEIGHT: f64 = 1.05;
pub const RAIL_INSET: f64 = 0.06;
/// The ladder up the front, just outside the deck edge.
pub const LADDER_X: f64 = 1.75;
pub const LADDER_HALF: f64 = 0.275;
pub const LADDER_Z: f64 = DECK_Z + 0.135;
pub const LADDER_TOP: f64 = DECK_TOP + RAIL_HEIGHT + 0.05;
/// Lowest point of the X-braces on the faces between the footings.
pub const FACE_BRACE_LOW: f64 = 1.4;
/// Clearance between neighbouring pieces' colliders.
const GAP: f64 = 0.005;

/// The parts a destroyed watchtower falls apart into. West is -x, front +z (the
/// door and ladder side).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TowerPiece {
    /// A pair of posts on one footing with the braces between them.
    WestBent,
    EastBent,
    /// The X-braces and girt between the footings on the back and front faces.
    BackBracing,
    FrontBracing,
    /// A half of the deck: beams, joists, planks and its railing.
    WestDeck,
    EastDeck,
    /// The cabin walls with their windows, awnings and (front) door.
    FrontWall,
    BackWall,
    WestWall,
    EastWall,
    /// The hipped roof with its anemometer.
    Roof,
    Ladder,
}

/// A box in tower coordinates (centre and full size, metres).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceBox {
    pub center: [f64; 3],
    pub size: [f64; 3],
}

const fn bounds(min: [f64; 3], max: [f64; 3]) -> PieceBox {
    PieceBox {
        center: [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ],
        size: [max[0] - min[0], max[1] - min[1], max[2] - min[2]],
    }
}

impl TowerPiece {
    pub const ALL: [TowerPiece; 12] = [
        TowerPiece::WestBent,
        TowerPiece::EastBent,
        TowerPiece::BackBracing,
        TowerPiece::FrontBracing,
        TowerPiece::WestDeck,
        TowerPiece::EastDeck,
        TowerPiece::FrontWall,
        TowerPiece::BackWall,
        TowerPiece::WestWall,
        TowerPiece::EastWall,
        TowerPiece::Roof,
        TowerPiece::Ladder,
    ];

    /// -1 for western and back pieces, +1 for eastern and front ones, 0 otherwise.
    pub fn side(self) -> f64 {
        match self {
            Self::WestBent
            | Self::BackBracing
            | Self::WestDeck
            | Self::BackWall
            | Self::WestWall => -1.0,
            Self::Roof => 0.0,
            _ => 1.0,
        }
    }

    /// The piece's main collider in tower coordinates; its model is centred on
    /// the box centre.
    pub fn bounds(self) -> PieceBox {
        let s = self.side();
        let wall = (DECK_TOP + GAP, CABIN_TOP - GAP);
        match self {
            Self::WestBent | Self::EastBent => {
                let x = s * TOWER_BASE.offset;
                let z = TOWER_BASE.post_z + POST / 2.0;
                bounds(
                    [x - POST / 2.0, POST_BREAK, -z],
                    [x + POST / 2.0, POST_TOP - GAP, z],
                )
            }
            Self::BackBracing | Self::FrontBracing => {
                let x = TOWER_BASE.offset - POST / 2.0 - 0.08;
                let z = s * TOWER_BASE.post_z;
                bounds(
                    [-x, FACE_BRACE_LOW, z - 0.25],
                    [x, POST_TOP - 0.25, z + 0.25],
                )
            }
            Self::WestDeck | Self::EastDeck => {
                let (x0, x1) = if s < 0.0 {
                    (-DECK_X, -GAP)
                } else {
                    (GAP, DECK_X)
                };
                bounds([x0, POST_TOP + GAP, -DECK_Z], [x1, DECK_TOP, DECK_Z])
            }
            Self::FrontWall | Self::BackWall => {
                let x = CABIN_X - CABIN_WALL - GAP;
                let z = s * (CABIN_Z - CABIN_WALL / 2.0);
                bounds(
                    [-x, wall.0, z - CABIN_WALL / 2.0],
                    [x, wall.1, z + CABIN_WALL / 2.0],
                )
            }
            Self::WestWall | Self::EastWall => {
                let x = s * (CABIN_X - CABIN_WALL / 2.0);
                bounds(
                    [x - CABIN_WALL / 2.0, wall.0, -CABIN_Z],
                    [x + CABIN_WALL / 2.0, wall.1, CABIN_Z],
                )
            }
            Self::Roof => {
                let (x, z) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
                bounds([-x, CABIN_TOP, -z], [x, roof_apex(), z])
            }
            Self::Ladder => bounds(
                [LADDER_X - LADDER_HALF - 0.03, GAP, LADDER_Z - 0.04],
                [LADDER_X + LADDER_HALF + 0.03, LADDER_TOP, LADDER_Z + 0.04],
            ),
        }
    }

    /// Further colliders on the piece's body, in tower coordinates: a deck half's
    /// railing runs and a cabin wall's propped awnings.
    pub fn extra_boxes(self) -> Vec<PieceBox> {
        let s = self.side();
        let rail = (DECK_TOP + GAP, DECK_TOP + RAIL_HEIGHT + 0.06);
        let (rx, rz) = (DECK_X - RAIL_INSET, DECK_Z - RAIL_INSET);
        let cabin = (CABIN_X, CABIN_Z);
        match self {
            Self::WestDeck | Self::EastDeck => {
                let (x0, x1) = if s < 0.0 { (-rx, -GAP) } else { (GAP, rx) };
                vec![
                    bounds([x0, rail.0, -rz - 0.06], [x1, rail.1, -rz + 0.06]),
                    bounds([x0, rail.0, rz - 0.06], [x1, rail.1, rz + 0.06]),
                    bounds(
                        [s * rx - 0.06, rail.0, -rz + 0.07],
                        [s * rx + 0.06, rail.1, rz - 0.07],
                    ),
                ]
            }
            Self::FrontWall | Self::BackWall => {
                let face = s * cabin.1;
                let (z0, z1) = if s < 0.0 {
                    (face - AWNING_REACH, face - 0.02)
                } else {
                    (face + 0.02, face + AWNING_REACH)
                };
                vec![bounds(
                    [-cabin.0 + 0.2, AWNING_BOTTOM, z0],
                    [cabin.0 - 0.2, AWNING_TOP, z1],
                )]
            }
            Self::WestWall | Self::EastWall => {
                let face = s * cabin.0;
                let (x0, x1) = if s < 0.0 {
                    (face - AWNING_REACH, face - 0.02)
                } else {
                    (face + 0.02, face + AWNING_REACH)
                };
                vec![bounds(
                    [x0, AWNING_BOTTOM, -cabin.1 + 0.2],
                    [x1, AWNING_TOP, cabin.1 - 0.2],
                )]
            }
            _ => Vec::new(),
        }
    }

    /// Hull points of the hipped roof (eave rectangle and ridge), for its convex
    /// collider; `None` for box-shaped pieces.
    pub fn hull(self) -> Option<Vec<[f64; 3]>> {
        (self == Self::Roof).then(|| {
            let (x, z) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
            let ridge = x - z;
            let top = CABIN_TOP + ROOF_EDGE;
            let mut points = Vec::new();
            for (px, pz) in [(x, z), (x, -z), (-x, z), (-x, -z)] {
                points.push([px, CABIN_TOP, pz]);
                points.push([px, top, pz]);
            }
            points.push([ridge, roof_apex(), 0.0]);
            points.push([-ridge, roof_apex(), 0.0]);
            points
        })
    }

    /// Long members fall as beams; boards, walls and the roof as panels.
    pub fn shape(self) -> FragmentShape {
        match self {
            Self::WestBent
            | Self::EastBent
            | Self::BackBracing
            | Self::FrontBracing
            | Self::Ladder => FragmentShape::Beam,
            _ => FragmentShape::Panel,
        }
    }

    /// Whether the piece is steel (the ladder) rather than timber.
    pub fn metal(self) -> bool {
        self == Self::Ladder
    }

    /// Whether the piece stands on the ground (legs and bracing kick out at the
    /// base) rather than riding on top of the frame.
    pub fn grounded(self) -> bool {
        matches!(
            self,
            Self::WestBent | Self::EastBent | Self::BackBracing | Self::FrontBracing
        )
    }
}

/// Height of the hip roof's ridge.
pub fn roof_apex() -> f64 {
    CABIN_TOP + ROOF_EDGE + ROOF_PITCH * (CABIN_Z + ROOF_OVERHANG)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: &PieceBox, b: &PieceBox) -> bool {
        (0..3).all(|i| (a.center[i] - b.center[i]).abs() < (a.size[i] + b.size[i]) / 2.0)
    }

    #[test]
    fn piece_colliders_start_apart() {
        let boxes: Vec<(TowerPiece, PieceBox)> = TowerPiece::ALL
            .iter()
            .flat_map(|&piece| {
                std::iter::once(piece.bounds())
                    .chain(piece.extra_boxes())
                    .map(move |b| (piece, b))
            })
            .collect();
        for (i, (a, box_a)) in boxes.iter().enumerate() {
            assert!(box_a.size.iter().all(|&s| s > 0.0), "{a:?} {box_a:?}");
            for (b, box_b) in &boxes[i + 1..] {
                if a != b {
                    assert!(!overlaps(box_a, box_b), "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn pieces_stay_clear_of_the_rubble_footings() {
        for piece in TowerPiece::ALL {
            let b = piece.bounds();
            if b.center[1] - b.size[1] / 2.0 >= TOWER_BASE.rubble_height {
                continue;
            }
            for side in [-1.0, 1.0] {
                let footing = PieceBox {
                    center: [
                        side * TOWER_BASE.offset,
                        TOWER_BASE.rubble_height / 2.0,
                        0.0,
                    ],
                    size: [TOWER_BASE.width, TOWER_BASE.rubble_height, TOWER_BASE.depth],
                };
                assert!(!overlaps(&b, &footing), "{piece:?}");
            }
        }
    }
}
