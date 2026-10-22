//! Meshes and materials of every effect pool: the translated Three.js effect
//! materials (`effect-materials.ts`, the TSL puffs/rings/tracks) and the effect
//! InstancedMeshes' geometries. Colors are sRGB hex like the originals.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use sloppy_core::geometry::{
    Mesh, cylinder_geometry, icosahedron_geometry, plane_geometry, tetrahedron_geometry,
};
use sloppy_core::scene::{Blending, Effect, Material, Side};
use sloppy_core::sim::PickupKind;
use sloppy_core::sim::ammunition::PROJECTILE_ORDER;
use sloppy_core::sim::data::pickup;

use super::PUFF_CAPACITY;
use super::explosions::DECAL_CAPACITY;
use super::laser::LASER_CAPACITY;
use super::leaves::{MAX_LEAVES, leaf_mesh};
use super::particles::{MAX_PARTICLES, shard_mesh};
use super::pickups::{
    MAX_PICKUP_EFFECTS, RING_CAPACITY, band_geometry, flash_geometry, glow_material,
    shockwave_geometry,
};
use super::pool::{PoolDesc, pool};
use super::projectiles::{PROJECTILE_CAPACITY, projectile_model};
use super::quarry_dust::QUARRY_DUST_CAPACITY;
use super::registry::{BLAST_RING, DEBRIS_CHIP, DUST, PUFF, STREAK, TRACK_MARK};
use super::rocket_smoke::ROCKET_SMOKE_CAPACITY;
use super::sparks::STREAK_CAPACITY;
use super::track_dust::TRACK_DUST_CAPACITY;
use super::track_gravel::TRACK_GRAVEL_CAPACITY;
use super::tracks::TRACK_CAPACITY;
use super::wood_chips::{MAX_STRAW, MAX_WOOD_CHIPS, splinter_mesh, straw_mesh};

/// Tread marks draw after the ground decals they lie on (`renderOrder = 1`),
/// and blast decals with them; smoke draws over both, glows over the smoke.
const TRACK_RENDER_ORDER: i32 = 1;
/// The smoke puffs' pool label, by which the browser finds it for its params.
pub const PUFF_POOL: &str = "blast puffs";
const PUFF_RENDER_ORDER: i32 = 2;
const STREAK_RENDER_ORDER: i32 = 3;
const TRACK_COLOR: u32 = 0x283222;
const TRACK_OPACITY: f32 = 0.38;
const GRAVEL_COLOR: u32 = 0xb09c7b;
const LASER_MOUNT_COLOR: u32 = 0x263b4c;
/// Ground decals (tread marks, blast rings) pull toward the camera to stay above
/// the coplanar floor (Three `polygonOffset` factor -1, units -1).
const DECAL_POLYGON_OFFSET: (f32, f32) = (-1.0, -1.0);

fn custom(name: &'static str) -> Effect {
    Effect::Custom {
        name,
        params: Vec::new(),
    }
}

/// `MeshBasicNodeMaterial({ transparent, depthWrite: false, fog: false })` with a
/// custom effect; instance tints supply color and opacity.
fn soft_billboard(effect: &'static str) -> Material {
    Material {
        transparent: true,
        depth_write: false,
        fog: false,
        effect: custom(effect),
        ..Material::basic(0xffffff)
    }
}

/// A flat quad in the XZ plane, facing up.
fn ground_quad() -> Mesh {
    let mut mesh = plane_geometry(1.0, 1.0);
    mesh.rotate_x(-FRAC_PI_2);
    mesh
}

/// Every effect pool, in `EffectSystems::for_each_pool` order.
pub fn pool_descs() -> Vec<PoolDesc> {
    let mut pools = Vec::new();
    let track = Material {
        transparent: true,
        opacity: TRACK_OPACITY,
        depth_write: false,
        polygon_offset: Some(DECAL_POLYGON_OFFSET),
        effect: custom(TRACK_MARK.name),
        ..Material::basic(TRACK_COLOR)
    };
    pools.push(PoolDesc {
        render_order: TRACK_RENDER_ORDER,
        ..PoolDesc::new("track marks", ground_quad(), track, TRACK_CAPACITY)
    });
    let dust_quad = Arc::new(plane_geometry(1.0, 1.0));
    let dust = Arc::new(soft_billboard(DUST.name));
    pools.push(pool("track dust", &dust_quad, &dust, TRACK_DUST_CAPACITY));
    let gravel = Material {
        flat_shading: true,
        ..Material::standard(GRAVEL_COLOR, 0.0, 1.0)
    };
    pools.push(PoolDesc::new(
        "track gravel",
        tetrahedron_geometry(1.0, 0),
        gravel,
        TRACK_GRAVEL_CAPACITY,
    ));
    pools.push(pool("quarry dust", &dust_quad, &dust, QUARRY_DUST_CAPACITY));

    let body = Arc::new(Material {
        vertex_colors: true,
        flat_shading: true,
        tone_mapped: false,
        ..Material::standard(0xffffff, 0.15, 0.6)
    });
    let team = Arc::new(Material {
        tone_mapped: false,
        ..Material::basic(0xffffff)
    });
    let flame = Arc::new(Material {
        vertex_colors: true,
        tone_mapped: false,
        ..Material::basic(0xffffff)
    });
    for kind in PROJECTILE_ORDER {
        let model = projectile_model(kind);
        pools.push(pool(
            "projectile bodies",
            &Arc::new(model.body),
            &body,
            PROJECTILE_CAPACITY,
        ));
        pools.push(pool(
            "projectile team bands",
            &Arc::new(model.team),
            &team,
            PROJECTILE_CAPACITY,
        ));
        if let Some(exhaust) = model.exhaust {
            pools.push(pool(
                "projectile flames",
                &Arc::new(exhaust),
                &flame,
                PROJECTILE_CAPACITY,
            ));
        }
    }
    pools.push(pool(
        "rocket smoke",
        &dust_quad,
        &dust,
        ROCKET_SMOKE_CAPACITY,
    ));

    let laser = pickup(PickupKind::Laser).color;
    let beam = |color: u32, opacity: f32| Material {
        transparent: opacity < 1.0,
        opacity,
        depth_write: opacity == 1.0,
        tone_mapped: false,
        ..Material::basic(color)
    };
    pools.push(PoolDesc::new(
        "laser halos",
        cylinder_geometry(0.045, 0.045, 1.0, 5),
        beam(laser, 0.5),
        LASER_CAPACITY,
    ));
    pools.push(PoolDesc::new(
        "laser cores",
        cylinder_geometry(0.014, 0.014, 1.0, 5),
        beam(0xffffff, 1.0),
        LASER_CAPACITY,
    ));
    pools.push(PoolDesc::new(
        "laser mounts",
        cylinder_geometry(0.15, 0.15, 0.12, 8),
        beam(LASER_MOUNT_COLOR, 1.0),
        LASER_CAPACITY,
    ));
    pools.push(PoolDesc::new(
        "laser lenses",
        icosahedron_geometry(0.105, 1),
        beam(laser, 1.0),
        LASER_CAPACITY,
    ));

    pools.push(PoolDesc::new(
        "pickup rings",
        shockwave_geometry(),
        glow_material(),
        RING_CAPACITY,
    ));
    pools.push(PoolDesc::new(
        "pickup bands",
        band_geometry(),
        glow_material(),
        MAX_PICKUP_EFFECTS,
    ));
    pools.push(PoolDesc::new(
        "pickup flashes",
        flash_geometry(),
        glow_material(),
        MAX_PICKUP_EFFECTS,
    ));

    // Faceted shards lit like the scene; sparky flecks and hot clods glow
    // through the chip effect's emissive.
    pools.push(PoolDesc::new(
        "particles",
        shard_mesh(),
        Material {
            flat_shading: true,
            effect: custom(DEBRIS_CHIP.name),
            ..Material::standard(0xffffff, 0.0, 0.85)
        },
        MAX_PARTICLES,
    ));
    // Lit like the crowns they fell from; the instance tint is the leaf's color.
    pools.push(PoolDesc {
        receive_shadow: true,
        ..PoolDesc::new(
            "leaves",
            leaf_mesh(),
            Material {
                side: Side::Double,
                ..Material::standard(0xffffff, 0.0, 0.9)
            },
            MAX_LEAVES,
        )
    });
    // Crate splinters, faceted and lit like the boards they came from; the
    // instance tint is the wood's color.
    pools.push(PoolDesc {
        receive_shadow: true,
        ..PoolDesc::new(
            "wood chips",
            splinter_mesh(),
            Material {
                flat_shading: true,
                ..Material::standard(0xffffff, 0.0, 0.85)
            },
            MAX_WOOD_CHIPS,
        )
    });
    pools.push(PoolDesc {
        receive_shadow: true,
        ..PoolDesc::new(
            "packing straw",
            straw_mesh(),
            Material {
                side: Side::Double,
                ..Material::standard(0xffffff, 0.0, 0.8)
            },
            MAX_STRAW,
        )
    });
    pools.push(PoolDesc {
        render_order: TRACK_RENDER_ORDER,
        ..PoolDesc::new(
            "blast rings",
            ground_quad(),
            Material {
                polygon_offset: Some(DECAL_POLYGON_OFFSET),
                ..soft_billboard(BLAST_RING.name)
            },
            DECAL_CAPACITY,
        )
    });
    pools.push(PoolDesc {
        render_order: PUFF_RENDER_ORDER,
        ..PoolDesc::new(
            PUFF_POOL,
            plane_geometry(2.0, 2.0),
            // Smoke takes the scene's aerial haze, so a distant plume recedes into
            // the air instead of standing as ink against it.
            Material {
                fog: true,
                ..soft_billboard(PUFF.name)
            },
            PUFF_CAPACITY,
        )
    });
    pools.push(PoolDesc {
        render_order: STREAK_RENDER_ORDER,
        ..PoolDesc::new(
            "streaks",
            plane_geometry(1.0, 1.0),
            Material {
                blending: Blending::Additive,
                tone_mapped: false,
                ..soft_billboard(STREAK.name)
            },
            STREAK_CAPACITY,
        )
    });
    pools
}
