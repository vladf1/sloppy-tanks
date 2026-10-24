//! The wrecks presentation keeps for the page: every vehicle kind, team and part.
//!
//! Building all forty took a third of a WebKit arena build (60-70 ms at iPad size)
//! for models no frame needs until the first tank dies. An arena's preparation builds
//! only [`SAMPLE_WRECK`], whose looks are every wreck's, so it compiles every wreck
//! pipeline and loads every wreck texture before the round; the page builds the rest
//! one per task once the arena is ready (`Presentation::prebuild_step`), and a death
//! before then builds its wreck on the spot.

use std::sync::Arc;

use sloppy_core::models::aged_wreck_material;
use sloppy_core::scene::Node;
use sloppy_core::sim::{Team, VehicleKind, WreckPart};

/// Every way a vehicle breaks up.
pub const WRECK_PARTS: [WreckPart; 5] = [
    WreckPart::Intact,
    WreckPart::Hull,
    WreckPart::Turret,
    WreckPart::TurretBarrel,
    WreckPart::Barrel,
];

/// The wreck built with the arena: a small one whose looks (pipelines and textures)
/// are those of every wreck (`tests::the_sample_has_every_wreck_look`).
pub const SAMPLE_WRECK: (VehicleKind, Team, WreckPart) =
    (VehicleKind::Scout, Team::Blue, WreckPart::Barrel);

/// Every wreck the library holds.
pub fn library_wrecks() -> impl Iterator<Item = (VehicleKind, Team, WreckPart)> {
    VehicleKind::ALL.into_iter().flat_map(|kind| {
        [Team::Blue, Team::Red]
            .into_iter()
            .flat_map(move |team| WRECK_PARTS.map(|part| (kind, team, part)))
    })
}

/// Give every material of a wreck the aging effect, which darkens base and
/// emissive color by the brightness each instance carries.
pub fn age_materials(node: &mut Node) {
    if let Some(drawable) = &mut node.drawable {
        drawable.material = Arc::new(aged_wreck_material(&drawable.material, 0.0));
    }
    for child in &mut node.children {
        age_materials(child);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sloppy_core::models::wreck_model;
    use sloppy_core::scene::{Effect, Node};

    use super::*;

    /// What of a part decides the pipelines it draws with (its variants, blending,
    /// depth and culling state, shadow casting) and the textures it loads; colours,
    /// roughness and effect parameters are uniforms.
    fn looks(node: &Node, out: &mut BTreeSet<String>) {
        if let Some(drawable) = &node.drawable {
            let m = &drawable.material;
            let effect = match &m.effect {
                Effect::None => "",
                Effect::Custom { name, .. } => name,
            };
            out.insert(format!(
                "{:?} {} {} {:?} {} {} {} {:?} {:?} {:?} {effect} {} {} {:?} {:?} {:?} {:?} {} {}",
                m.shading,
                m.alpha_test > 0.0,
                m.alpha_to_coverage,
                m.blending,
                m.transparent,
                m.depth_test,
                m.depth_write,
                m.polygon_offset,
                m.side,
                m.shadow_side,
                drawable.cast_shadow,
                drawable.receive_shadow,
                m.map,
                m.emissive_map,
                m.bump_map,
                m.extra_textures,
                m.vertex_colors,
                m.flat_shading,
            ));
        }
        for child in &node.children {
            looks(child, out);
        }
    }

    fn wreck_looks((kind, team, part): (VehicleKind, Team, WreckPart)) -> BTreeSet<String> {
        let mut source = (*wreck_model(kind, team, part)).clone();
        age_materials(&mut source);
        let mut out = BTreeSet::new();
        looks(&source, &mut out);
        out
    }

    /// A wreck with a look the sample lacks would compile its pipeline, or load its
    /// texture, at the first death that shows it, mid-round.
    #[test]
    fn the_sample_has_every_wreck_look() {
        let sample = wreck_looks(SAMPLE_WRECK);
        assert!(!sample.is_empty());
        for wreck in library_wrecks() {
            let missing: Vec<_> = wreck_looks(wreck).difference(&sample).cloned().collect();
            assert!(
                missing.is_empty(),
                "{wreck:?} has looks the sample lacks: {missing:?}"
            );
        }
    }

    #[test]
    fn the_library_holds_every_wreck_once() {
        let all: BTreeSet<_> = library_wrecks()
            .map(|(kind, team, part)| (kind as u8, team as u8, part as u8))
            .collect();
        assert_eq!(all.len(), VehicleKind::ALL.len() * 2 * WRECK_PARTS.len());
        let (kind, team, part) = SAMPLE_WRECK;
        assert!(library_wrecks().any(|wreck| wreck == (kind, team, part)));
    }
}
