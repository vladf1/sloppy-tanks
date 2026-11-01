//! The game data the model builders read, kept apart from the balance in `data` so a
//! balance change leaves the page's cached models (`sloppy_geometry_only`) alone: the
//! arena's size, the team colours and each vehicle's model scale.

use super::kinds::VehicleKind;

/// Half the playable square's side (m).
pub const ARENA: f64 = 60.0;

pub const TEAM_COLORS: [u32; 2] = [0x008cff, 0xff303e];

/// Uniform model scale: real vehicle proportions fitted to the arena's 1.95 m reference
/// width. Comparable game sizes; Bruiser anchors the fleet at 1.95 units wide.
pub const fn vehicle_scale(kind: VehicleKind) -> f64 {
    match kind {
        VehicleKind::Scout => (3.59 / 2.3) * (1.95 / 3.66),
        VehicleKind::Balanced => 1.95 / 2.42,
        VehicleKind::Heavy => (3.5 / 2.5) * (1.95 / 3.66) * 1.15,
        VehicleKind::Humvee => 0.9,
    }
}
