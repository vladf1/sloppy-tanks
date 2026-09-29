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

// Scenery (village, harbor, quarry, extra-level floors).
mod concrete_surfaces;
pub mod effects_scenery;
mod ground_surfaces;
mod harbor_models;
mod harbor_scenery;
mod harbor_surfaces;
mod harbor_vessels;
mod harbor_water;
mod house_surfaces;
mod loading_assets;
pub mod pending_scenery;
mod quarry_benches;
mod quarry_machinery;
mod quarry_ramp;
mod quarry_rock_shape;
mod quarry_scenery;
mod quarry_scree;
mod quarry_site_details;
mod quarry_soil;
mod quarry_surfaces;
mod quarry_terrain;
mod scenery;
mod village_atmosphere;
mod village_landmarks;
mod village_landscape;
mod village_roads;
mod village_scenery;
mod village_vegetation;
mod water_surface;

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

pub use concrete_surfaces::{CONCRETE_TEXTURE, concrete_material, concrete_wall};
pub use ground_surfaces::{GroundKind, ground_material, ground_texture, ground_uvs, road_geometry};
pub use harbor_models::{CARGO_SPLIT, CargoShape, CrateShape, cargo_stack, shipping_container};
pub use harbor_scenery::HarborScenery;
pub use harbor_surfaces::{HarborSurface, harbor_box, harbor_material, steel_box};
pub use harbor_vessels::{HarborFleet, harbor_beam};
pub use harbor_water::{HARBOR_WATER_HEIGHT, harbor_water};
pub use house_surfaces::{
    HouseSurface, house_material, house_texture, shingle_roof, siding_box, siding_gable,
};
pub use loading_assets::node_textures;
pub use quarry_benches::{
    ButteSpot, ScreeSpot, StockpileSpot, TalusStrip, quarry_bench, quarry_butte,
    quarry_butte_footprint, quarry_butte_spot, quarry_scree_spots, quarry_stockpile_geometry,
    quarry_stockpile_reach, quarry_stockpile_spot, quarry_talus_geometry, quarry_talus_point,
    quarry_talus_strips,
};
pub use quarry_machinery::{quarry_dump_truck, quarry_excavator};
pub use quarry_ramp::{
    QUARRY_RAMP, QuarryRamp, RampRock, quarry_ramp_boulders, quarry_ramp_geometry,
    quarry_ramp_height, quarry_ramp_spoil,
};
pub use quarry_rock_shape::{RockShape, quarry_rock_shape, quarry_rock_variant};
pub use quarry_scenery::{QuarryScenery, SpawnPadPiece, SpawnPadShape, quarry_spawn_pad_pieces};
pub use quarry_scree::{quarry_scree, quarry_scree_geometry, quarry_scree_rubble};
pub use quarry_soil::{
    ACCUM_CELLS, QUARRY_SOIL_SIZE, QUARRY_TERRAIN_EXTENT, bake_quarry_soil, quarry_soil_pixels,
};
pub use quarry_surfaces::{
    RubbleStone, roughen_stone, sand_drift_material, sandstone_footing, sandstone_material,
    sandstone_rock, sandstone_rubble,
};
pub use quarry_terrain::{
    QUARRY_FLOOR, plain_soil_colors, quarry_soil_texture, quarry_terrain, sand_accum, soil_material,
};
pub use scenery::{
    MapTheme, SHADOW_DEPTH, Scenery, ShadowBox, build_scenery, create_arena_floor,
    create_spawn_pads, custom_floor, custom_spawn_pads, default_sun_shadow, fit_sun_shadow,
    lighting,
};
pub use village_atmosphere::{SmokeCover, set_chimney_smoke, village_atmosphere};
pub use village_landmarks::{WATERWHEEL, village_landmarks};
pub use village_landscape::{
    CREEK_HEIGHT, creek_distance, valley_height, valley_height_at, village_landscape,
};
pub use village_roads::{ROAD_SHOULDER, VillageRoad, is_village_dirt, village_roads};
pub use village_scenery::{CHIMNEY_SMOKE_NODE, VillageScenery};
pub use village_vegetation::{MEADOW_FLOWERS, MEADOW_TUFTS, village_vegetation};
pub use water_surface::{WATER_NORMALS, WaterKind, water_material, water_surface};

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
pub use pickup_visuals::{
    PICKUP_ATLAS_PADDING, PICKUP_ATLAS_PATH, PICKUP_ATLAS_SIZE, PICKUP_ATLAS_STRIDE,
    PICKUP_ICON_SIZE, PickupKind, pickup_atlas_uv, pickup_cube,
};
pub use prop_support::Random;
pub use quarry_barriers::{dragon_tooth, steel_hedgehog};
pub use quarry_shapes::{
    HEDGEHOG_BEAMS, HedgehogBeam, ToothProfile, dragon_tooth_point, dragon_tooth_profile,
    dragon_tooth_variant,
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
