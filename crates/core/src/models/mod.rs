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
//!   levels use [`create_arena_floor`] and [`create_spawn_pads`].
//! - Cover: [`cover_model`] from a [`CoverShape`] (convertible from
//!   [`RenderCover`](crate::sim::render_state::RenderCover)); rebuild when
//!   [`cover_damage_stage`] or the timber hit count changes. Trees:
//!   [`tree_model`], whose boughs drop by [`tree_branch_stage`] /
//!   [`branch_drop_stage`] (node names in [`tree_part`]). Detached timber members:
//!   [`timber_part_model`].
//! - Props: [`pickup_cube`], [`flags_model`] (cloth node [`FLAG_CLOTH_NODE`]),
//!   debris meshes [`barrel_scrap_geometry`] and [`trunk_fragment`], burnt wrecks
//!   darkened with [`aged_wreck_material`].
//! - Custom shading: every `Effect::Custom` name with its parameters and inputs is
//!   documented in [`effects_props`] and [`effects_scenery`]; the renderer's WGSL
//!   implements them.
//! - Textures: file paths are in each `TextureRef`; [`node_textures`] lists what a
//!   tree needs, and presentation supplies its generated keys. Those are baked in
//!   row bands by [`bake_quarry_soil`] (the quarry soil) or drawn by the browser
//!   from [`effects_scenery::canvas_texture`] (signs and labels).

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
pub use batching::{batch, is_paintable, painted, vertex_material};
pub use model_primitives::{DEFAULT_BOX_RADIUS, TEAM_COLORS, cylinder_part, paint, put, shadowed};

// Vehicles.
pub use humvee_model::HUMVEE_BODY_LENGTH_SCALE;
pub use tank_dimensions::{TankDimensions, tank_dimensions};
pub use tank_model::{tank_model, tank_model_variant};
pub use tank_surfaces::armor_wear_texture;
pub use wreck_model::{WreckPart, wreck_model};

// Cover, trees and props.
pub use barrel_debris::{BarrelScrap, barrel_scrap_geometry};
pub use cover_model::{CoverShape, cover_damage_stage, cover_model};
pub use effects_props::{aged_wreck_material, wreck_brightness};
pub use flags::{FLAG_CLOTH_NODE, flags_model};
pub use pickup_visuals::pickup_cube;
pub use timber_model::timber_part_model;
pub use tower_model::tower_piece_model;
pub use tree_models::{
    TREE_FAMILIES, TreeDetail, TreeFoliage, TreeShape, branch_drop_stage, tree_branch_stage,
    tree_foliage, tree_model, tree_part, trunk_fragment,
};

// Surfaces.
pub use concrete_surfaces::{CONCRETE_TEXTURE, concrete_material};
pub use ground_surfaces::GroundKind;
pub use house_surfaces::{HouseSurface, house_texture, siding_box};
pub use water_surface::water_normals;

// Map scenery.
pub use harbor_scenery::HarborScenery;
pub use loading_assets::node_textures;
pub use quarry_scenery::{SpawnPadShape, quarry_spawn_pad_pieces};
pub use quarry_soil::{QUARRY_SOIL_SIZE, bake_quarry_soil};
pub use quarry_terrain::sand_accum;
pub use scenery::{MapTheme, Scenery, build_scenery, create_arena_floor, create_spawn_pads};
pub use village_atmosphere::{CHIMNEY_SMOKE_NODE, SmokeCover};
pub use village_landmarks::WATERWHEEL;
pub use village_roads::is_village_dirt;
pub use village_scenery::VillageScenery;

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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_props;
#[cfg(test)]
mod tests_scenery;
