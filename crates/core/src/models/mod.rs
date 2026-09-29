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

// Cover, tree and prop models.
mod barrel_debris;
mod barrel_surfaces;
mod cottage_details;
mod cover_model;
pub mod effects_props;
mod flags;
mod harbor_models;
mod pending_props;
mod pickup_visuals;
mod prop_support;
mod quarry_barriers;
mod quarry_shapes;
mod timber_model;
mod tree_debris;
mod tree_models;

pub use barrel_debris::{BarrelScrap, barrel_scrap_geometry};
pub use barrel_surfaces::{PAINTED_DRUM_TEXTURE, explosive_barrel};
pub use batching::paint_mesh;
pub use cottage_details::cottage_details;
pub use cover_model::{
    CoverKind, CoverModel, CoverShape, cover_damage_stage, cover_model, tower_base,
};
pub use effects_props::{aged_wreck_material, debris_fade_material, wreck_brightness};
pub use flags::{
    FLAG_CLOTH_BOUNDS_RADIUS, FLAG_CLOTH_NODE, FLAG_POLE, flag_phase, flag_positions, flags_model,
};
pub use harbor_models::{CargoShape, cargo_stack, shipping_container};
pub use pickup_visuals::{
    PICKUP_ATLAS_PADDING, PICKUP_ATLAS_PATH, PICKUP_ATLAS_SIZE, PICKUP_ATLAS_STRIDE,
    PICKUP_ICON_SIZE, PickupKind, pickup_atlas_uv, pickup_cube,
};
pub use prop_support::Random;
pub use quarry_barriers::{dragon_tooth, steel_hedgehog};
pub use quarry_shapes::{
    HEDGEHOG_BEAMS, HedgehogBeam, RockShape, ToothProfile, dragon_tooth_point,
    dragon_tooth_profile, dragon_tooth_variant, quarry_rock_shape, quarry_rock_variant,
};
pub use timber_model::{
    TIMBER_HEALTH, TimberFace, TimberHit, TimberJoin, TimberMark, TimberPart, TimberPartKind,
    TimberWall, add_timber_parts, timber_damage_stage, timber_part_model, timber_parts,
};
pub use tree_debris::{
    FALLING_BRANCH_LIFETIME, MAX_FALLING_BRANCHES, fading_material, falling_branch_model,
};
pub use tree_models::{
    TREE_FAMILIES, TreeDetail, TreeModel, TreeProportions, TreeShape, branch_drop_stage,
    set_tree_damage, set_tree_destroyed, tree_branch_stage, tree_model, tree_part,
    tree_proportions, trunk_fragment,
};

#[cfg(test)]
mod tests_props;
