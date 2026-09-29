//! Model builders: CPU node trees for the renderer to draw and the simulation to
//! measure. Ports of the TypeScript model files, one module per file.
//!
//! Vehicle models (`tank_model`, `humvee_model`, `wreck_model`) name the parts that
//! presentation animates and measurement reads; see [`part`]. Models are built
//! once per variant and shared as `Arc<Node>`: clone the node to pose its parts.

mod batching;
mod humvee_model;
mod model_primitives;
mod tank_dimensions;
mod tank_model;
mod tank_surfaces;
mod wreck_model;

pub use batching::batch;
pub use humvee_model::{HUMVEE_BODY_LENGTH_SCALE, humvee_model};
pub use model_primitives::{
    CYLINDER_METALNESS, DEFAULT_BOX_RADIUS, DEFAULT_METALNESS, DEFAULT_ROUGHNESS, TEAM_COLORS,
    box_part, cylinder_part, material, paint, put, rotated, shadow_receiver, shadowed,
};
pub use tank_dimensions::{
    HUMVEE_COMBAT_MUZZLE_Y, TankDimensions, tank_dimensions, tank_hull, tank_muzzle,
    tank_visual_muzzle,
};
pub use tank_model::{tank_model, tank_model_variant};
pub use tank_surfaces::{ARMOR_WEAR_TEXTURE, apply_tank_surface, armor_wear_texture};
pub use wreck_model::{WreckPart, wreck_model};

/// Names of the vehicle parts the TypeScript kept in `userData`. Every vehicle
/// model (tanks and Humvee) has all of them.
pub mod part {
    /// The chassis group (TS `userData.hull`): tilts with suspension; its mesh
    /// bounds are the vehicle's hull hit box.
    pub const HULL: &str = "hull";
    /// The turret group (TS `userData.turret`): yaws with aim; first-person eye
    /// positions are in its frame.
    pub const TURRET: &str = "turret";
    /// The gun group inside the turret (TS `userData.barrel`): slides back on recoil.
    pub const BARREL: &str = "barrel";
    /// The group inside the hull (TS `userData.trackGroup`) that scrolls with
    /// driving: the top treads of tanks, the wheels of the Humvee.
    pub const TRACK_GROUP: &str = "track-group";
    /// The launch point inside the barrel (TS `userData.muzzle`): the bore disc
    /// mesh of tanks, an empty marker for the Humvee.
    pub const MUZZLE: &str = "muzzle";
}

/// A team index (TS `Team`, 0 or 1), indexing [`TEAM_COLORS`].
pub type Team = u8;

pub use crate::sim::types::VehicleKind;

impl VehicleKind {
    /// The TypeScript identifier, also the model root's node name.
    pub fn name(self) -> &'static str {
        self.as_str()
    }

    /// Uniform model scale (`VEHICLES[kind].scale`): real vehicle proportions fitted
    /// to the arena's 1.95 m reference width.
    pub fn scale(self) -> f64 {
        crate::sim::data::vehicle(self).scale
    }
}

#[cfg(test)]
mod tests;
