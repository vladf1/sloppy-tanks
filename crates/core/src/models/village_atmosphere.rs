//! Port of `village-atmosphere.ts`: bounded, GPU-animated chimney wisps. One
//! instanced quad per wisp; the renderer animates and billboards them
//! ([`effects_scenery::CHIMNEY_SMOKE`]).

use std::sync::Arc;

use glam::DMat4;

use crate::geometry::{Attribute, plane_geometry};
use crate::scene::{Effect, Instance, Material, Node};

use super::effects_scenery::{CHIMNEY_SMOKE, SMOKE_ORIGIN, SMOKE_PHASE};

/// Chimneys that can smoke at once, and wisps per chimney.
pub const SMOKE_SOURCES: usize = 24;
pub const WISPS_PER_SOURCE: usize = 8;
const WISP_CAPACITY: usize = SMOKE_SOURCES * WISPS_PER_SOURCE;
/// The watermill chimney always smokes.
const MILL_CHIMNEY: [f64; 3] = [-32.0, 11.5, -71.0];

/// The cover fields the smoke sources read (`Pick<Cover, "kind" | "destructible" |
/// "x" | "z" | "w" | "h" | "d">`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeCover<'a> {
    pub kind: &'a str,
    pub destructible: bool,
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
}

/// `new VillageAtmosphere().mesh` (`village-chimney-smoke`): a unit quad with the
/// per-instance [`SMOKE_ORIGIN`] and [`SMOKE_PHASE`] attributes and no wisps yet;
/// call [`set_chimney_smoke`] on reset. Never frustum culled.
pub fn village_atmosphere() -> Node {
    let mut quad = plane_geometry(1.0, 1.0);
    quad.set_attribute(Attribute::instance(
        SMOKE_ORIGIN,
        3,
        vec![0.0; WISP_CAPACITY * 3],
    ));
    quad.set_attribute(Attribute::instance(
        SMOKE_PHASE,
        1,
        (0..WISP_CAPACITY)
            .map(|i| ((i % 8) as f64 / 8.0 + (i / 8) as f64 * 0.013) as f32)
            .collect(),
    ));
    let material = Material {
        transparent: true,
        depth_write: false,
        fog: false,
        effect: Effect::Custom {
            name: CHIMNEY_SMOKE,
            params: Vec::new(),
        },
        ..Material::basic(0xffffff)
    };
    let mut node = Node::mesh(Arc::new(quad), Arc::new(material));
    node.name = "village-chimney-smoke".into();
    let drawable = node.drawable.as_mut().expect("smoke mesh");
    drawable.frustum_culled = false;
    drawable.instances = Some(Vec::new());
    node
}

/// `VillageAtmosphere.setCovers(covers)`: the mill chimney plus every
/// indestructible house's chimney (up to [`SMOKE_SOURCES`]), eight wisps each. The
/// instance list only sets the drawn instance count; its matrices are identity.
pub fn set_chimney_smoke(smoke: &mut Node, covers: &[SmokeCover]) {
    let sources: Vec<[f64; 3]> = std::iter::once(MILL_CHIMNEY)
        .chain(
            covers
                .iter()
                .filter(|c| c.kind == "house" && !c.destructible)
                .map(|c| [c.x - c.w * 0.25, c.h + 0.3, c.z - c.d * 0.2]),
        )
        .collect();
    let drawable = smoke.drawable.as_mut().expect("smoke mesh");
    let mut mesh = (*drawable.mesh).clone();
    let origins = mesh
        .attributes
        .iter_mut()
        .find(|attribute| attribute.name == SMOKE_ORIGIN)
        .expect("smoke origins");
    for (i, p) in sources.iter().take(SMOKE_SOURCES).enumerate() {
        for j in 0..WISPS_PER_SOURCE {
            let at = (i * WISPS_PER_SOURCE + j) * 3;
            origins.data[at..at + 3].copy_from_slice(&p.map(|v| v as f32));
        }
    }
    drawable.mesh = Arc::new(mesh);
    let count = sources.len().min(SMOKE_SOURCES) * WISPS_PER_SOURCE;
    drawable.instances = Some(vec![
        Instance {
            matrix: DMat4::IDENTITY,
            color: None,
        };
        count
    ]);
}
