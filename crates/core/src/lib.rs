//! The one implementation of Sloppy Tanks' rules, shared by the browser (Wasm) and
//! the native multiplayer server. Nothing here touches a GPU, a DOM or a socket.
//!
//! - `sim`: authoritative fixed-step simulation on Rapier, bots, navigation, combat.
//! - `geometry` / `scene` / `models`: CPU meshes and model trees. Rendering uploads
//!   them; the simulation measures them (tank hulls, rock and barrier hulls).
//! - `net`: protocol, replication and room hosting, independent of the transport.

pub mod geometry;
pub mod models;
pub mod net;
pub mod scene;
pub mod sim;
