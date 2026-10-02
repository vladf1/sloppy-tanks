//! Model builders: CPU node trees for the renderer to draw and the simulation to
//! measure. Ports of the TypeScript model files, one module per file.
//!
//! Models never own gameplay rules: kinds, layouts, shapes shared with collision
//! (rocks, dragon's teeth, hedgehogs), timber members, tree proportions and the
//! seeded [`Random`](crate::sim::math::Random) stream come from [`crate::sim`].
//! Cached geometry and materials are `Arc`s shared across rounds; callers own only
//! transforms.
//!
//! # Entry points for presentation
//!
//! - Vehicles: [`tank_model`] (tanks and the Humvee) and [`wreck_model`], shared
//!   `Arc<Node>`s whose animated joints are named in [`part`]. Measurements for the
//!   simulation: [`tank_dimensions`].
//! - Map scenery: [`build_scenery`]`(theme)` once per theme, then
//!   [`Scenery::update`] every frame (animated parts: [`WATERWHEEL`], the harbor
//!   beacons, [`CHIMNEY_SMOKE_NODE`] fed by [`VillageScenery::set_covers`]). Extra
//!   levels use [`custom_floor`] and [`custom_spawn_pads`]. Sun and fill settings are
//!   in [`lighting`] with [`default_sun_shadow`] and [`fit_sun_shadow`].
//! - Cover: [`cover_model`] from a [`CoverShape`] (convertible from
//!   [`RenderCover`](crate::sim::render_state::RenderCover)); rebuild when
//!   [`cover_damage_stage`] or the timber hit count changes. Trees:
//!   [`tree_model`], damaged with [`set_tree_damage`] / [`set_tree_destroyed`]
//!   (node names in [`tree_part`]); shed boughs fall as [`falling_branch_model`].
//!   Detached timber members: [`timber_part_model`].
//! - Props: [`pickup_cube`], [`flags_model`] (cloth node [`FLAG_CLOTH_NODE`], pole
//!   [`FLAG_POLE`]), debris meshes [`barrel_scrap_geometry`] and [`trunk_fragment`],
//!   burnt wrecks darkened with [`aged_wreck_material`], fading debris with
//!   [`debris_fade_material`].
//! - Custom shading: every `Effect::Custom` name with its WGSL contract is
//!   documented in [`effects_props`] and [`effects_scenery`].
//! - Textures: file paths are in each `TextureRef`; [`node_textures`] lists what a
//!   tree needs (for preloading). Generated keys are baked by
//!   [`effects_scenery::generated_texture`] (the quarry soil) or drawn by the
//!   browser from [`effects_scenery::canvas_texture`] (signs and labels).

mod batching;
mod model_primitives;

// Vehicles.
mod humvee_model;
mod tank_details;
mod tank_dimensions;
mod tank_kit;
mod tank_model;
mod tank_surfaces;
mod wreck_model;

// Cover, trees and props.
mod barrel_debris;
mod barrel_surfaces;
mod cover_model;
pub mod effects_props;
mod flags;
mod harbor_models;
mod pickup_visuals;
mod quarry_barriers;
mod timber_model;
mod tower_model;
mod tree_debris;
mod tree_models;

// Surfaces shared by cover and scenery.
mod building_kit;
mod concrete_surfaces;
mod ground_surfaces;
mod harbor_surfaces;
mod house_model;
mod house_surfaces;
mod quarry_surfaces;
mod water_surface;

// Map scenery (village, harbor, quarry, extra-level floors).
pub mod effects_scenery;
mod harbor_scenery;
mod harbor_vessels;
mod harbor_water;
mod loading_assets;
mod quarry_benches;
mod quarry_machinery;
mod quarry_ramp;
mod quarry_scenery;
mod quarry_scree;
mod quarry_site_details;
mod quarry_soil;
mod quarry_terrain;
mod scenery;
mod village_atmosphere;
mod village_landmarks;
mod village_landscape;
mod village_roads;
mod village_scenery;
mod village_vegetation;

// Shared building blocks.
pub use batching::{batch, paint_mesh, vertex_material};
pub use model_primitives::{
    CYLINDER_METALNESS, DEFAULT_BOX_RADIUS, DEFAULT_METALNESS, DEFAULT_ROUGHNESS, TEAM_COLORS,
    adopt_children, apply_matrix_to_node, box_part, cylinder_part, material, paint, put, rotated,
    shadow_receiver, shadowed, span_between,
};

// Vehicles.
pub use humvee_model::{HUMVEE_BODY_LENGTH_SCALE, humvee_model};
pub use tank_dimensions::{
    HUMVEE_COMBAT_MUZZLE_Y, TankDimensions, tank_dimensions, tank_hull, tank_muzzle,
    tank_visual_muzzle,
};
pub use tank_model::{tank_model, tank_model_variant};
pub use tank_surfaces::{ARMOR_WEAR_TEXTURE, apply_tank_surface, armor_wear_texture};
pub use wreck_model::{WreckPart, wreck_model};

// Cover, trees and props.
pub use barrel_debris::{BarrelScrap, barrel_scrap_geometry};
pub use barrel_surfaces::{PAINTED_DRUM_TEXTURE, explosive_barrel};
pub use cover_model::{CoverModel, CoverShape, cover_damage_stage, cover_model};
pub use effects_props::{aged_wreck_material, debris_fade_material, wreck_brightness};
pub use flags::{
    FLAG_CLOTH_BOUNDS_RADIUS, FLAG_CLOTH_NODE, FLAG_POLE, flag_phase, flag_positions, flags_model,
};
pub use harbor_models::{CARGO_SPLIT, CargoShape, CrateShape, cargo_stack, shipping_container};
pub use pickup_visuals::{
    PICKUP_ATLAS_PADDING, PICKUP_ATLAS_PATH, PICKUP_ATLAS_SIZE, PICKUP_ATLAS_STRIDE,
    PICKUP_ICON_SIZE, pickup_atlas_tile, pickup_atlas_uv, pickup_cube,
};
pub use quarry_barriers::{dragon_tooth, steel_hedgehog};
pub use timber_model::{add_timber_parts, timber_part_model};
pub use tower_model::tower_piece_model;
pub use tree_debris::{fading_material, falling_branch_model};
pub use tree_models::{
    TREE_FAMILIES, TreeDetail, TreeFoliage, TreeModel, TreeShape, branch_drop_stage,
    set_tree_damage, set_tree_destroyed, tree_branch_stage, tree_foliage, tree_model, tree_part,
    trunk_fragment,
};

// Surfaces.
pub use concrete_surfaces::{CONCRETE_TEXTURE, concrete_material, concrete_wall};
pub use ground_surfaces::{
    GroundKind, ground_material, ground_texture, ground_texture_path, ground_uvs, road_geometry,
};
pub use harbor_surfaces::{HarborSurface, harbor_box, harbor_material, steel_box};
pub use house_surfaces::{
    HouseSurface, house_material, house_texture, shingle_roof, siding_box, siding_gable,
};
pub use quarry_surfaces::{
    RubbleStone, SANDSTONE_TEXTURE, roughen_stone, sand_drift_material, sandstone_footing,
    sandstone_material, sandstone_rock, sandstone_rubble,
};
pub use water_surface::{WATER_NORMALS, WaterKind, water_material, water_normals, water_surface};

// Map scenery.
pub use harbor_scenery::HarborScenery;
pub use harbor_vessels::{HarborFleet, harbor_beam};
pub use harbor_water::{HARBOR_WATER_HEIGHT, harbor_water};
pub use loading_assets::node_textures;
pub use quarry_benches::{
    ButteSpot, ScreeSpot, StockpileSpot, TalusStrip, quarry_bench, quarry_butte, quarry_butte_spot,
    quarry_scree_spots, quarry_stockpile_geometry, quarry_stockpile_reach, quarry_stockpile_spot,
    quarry_talus_geometry, quarry_talus_point, quarry_talus_strips,
};
pub use quarry_machinery::{quarry_dump_truck, quarry_excavator};
pub use quarry_ramp::{
    QUARRY_RAMP, QuarryRamp, RampRock, quarry_ramp_boulders, quarry_ramp_geometry,
    quarry_ramp_height, quarry_ramp_spoil,
};
pub use quarry_scenery::{QuarryScenery, SpawnPadPiece, SpawnPadShape, quarry_spawn_pad_pieces};
pub use quarry_scree::{quarry_scree, quarry_scree_geometry, quarry_scree_rubble};
pub use quarry_soil::{
    ACCUM_CELLS, QUARRY_SOIL_SIZE, QUARRY_TERRAIN_EXTENT, bake_quarry_soil, quarry_soil_pixels,
};
pub use quarry_terrain::{
    QUARRY_FLOOR, plain_soil_colors, quarry_soil_texture, quarry_terrain, sand_accum, soil_material,
};
pub use scenery::{
    MapTheme, SHADOW_DEPTH, Scenery, ShadowBox, build_scenery, create_arena_floor,
    create_spawn_pads, custom_floor, custom_spawn_pads, default_sun_shadow, fit_sun_shadow,
};
pub use village_atmosphere::{
    SMOKE_SOURCES, SmokeCover, WISPS_PER_SOURCE, set_chimney_smoke, village_atmosphere,
};
pub use village_landmarks::{WATERWHEEL, village_landmarks};
pub use village_landscape::{
    CREEK_HEIGHT, creek_distance, valley_height, valley_height_at, village_landscape,
};
pub use village_roads::{ROAD_SHOULDER, VillageRoad, is_village_dirt, village_roads};
pub use village_scenery::{CHIMNEY_SMOKE_NODE, VillageScenery};
pub use village_vegetation::{MEADOW_FLOWERS, MEADOW_TUFTS, village_vegetation};

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

/// The side a vehicle, flag or spawn pad belongs to; indexes [`TEAM_COLORS`].
pub use crate::sim::types::Team;

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
#[cfg(test)]
mod tests_props;
#[cfg(test)]
mod tests_scenery;
