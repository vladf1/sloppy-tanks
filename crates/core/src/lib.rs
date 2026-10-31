//! The one implementation of Sloppy Tanks' rules, shared by the browser (Wasm) and
//! the native multiplayer server. Nothing here touches a GPU, a DOM or a socket.
//!
//! - `sim`: authoritative fixed-step simulation on Rapier, bots, navigation, combat.
//! - `geometry` / `scene` / `models`: CPU meshes and model trees. Rendering uploads
//!   them; the simulation measures them (tank hulls, rock and barrier hulls).
//! - `net`: protocol, replication and room hosting, independent of the transport.

// The geometry-only build leaves the callers of much of what it compiles out.
#![cfg_attr(sloppy_geometry_only, allow(dead_code, unused_imports))]

/// Items the page's geometry cache does not reach, left out of a build with
/// `--cfg sloppy_geometry_only`: that build compiles only what the cached model builders
/// (`models::build_scenery`, `models::tank_model_variant`) and their codec can reach, so a
/// builder that comes to depend on anything else fails it, and the cache's key
/// (`geometrySources` in `scripts/content-version.mjs`) hashes exactly the files it
/// compiles.
macro_rules! beyond_geometry {
    ($($item:item)*) => {
        $(
            #[cfg(not(sloppy_geometry_only))]
            $item
        )*
    };
}

pub mod geometry;
pub mod models;
pub mod scene;
pub mod scene_codec;
pub mod sim;

beyond_geometry! {
    pub mod net;
}
