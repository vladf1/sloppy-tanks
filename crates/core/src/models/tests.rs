//! Vehicle model checks. The hull bounds and muzzles still match what the
//! TypeScript `tankHull`/`tankVisualMuzzle` measured in Node: the simulation sizes
//! the tank's collider and fires from them. Detail is bounded by budgets.

use std::sync::Arc;

use glam::DVec3;

use super::*;
use crate::scene::{Node, TextureSource};

/// (drawn parts, triangles) over every drawable in the tree.
fn counts(node: &Node) -> (usize, usize) {
    let mut totals = (0, 0);
    node.traverse(glam::DMat4::IDENTITY, &mut |part, _| {
        if let Some(drawable) = &part.drawable {
            totals.0 += 1;
            totals.1 += drawable.mesh.triangle_count();
        }
    });
    totals
}

#[test]
fn tank_dimensions_are_measured_from_the_model() {
    let expected = [
        (
            VehicleKind::Scout,
            [0.0, 0.18420179971489656, 0.0],
            [1.9127049249722883, 0.6960582679971488, 3.7155932591971026],
            [0.0, 0.7068692088382038, 3.0977503563791875],
            [0.0, 0.7068692088382038, 3.0977503563791875],
        ),
        (
            VehicleKind::Balanced,
            [0.0, 0.2066838842975206, 0.0],
            [1.9500000067239949, 0.7308471074380165, 4.27091664998905],
            [0.0, 0.7735537190082644, 3.098243801652892],
            [0.0, 0.7735537190082644, 3.098243801652892],
        ),
        (
            VehicleKind::Heavy,
            [0.0, 0.22860020491803268, 0.0],
            [2.144467220272685, 0.7951684426229506, 4.702433035247105],
            [0.0, 0.8406311475409833, 3.812862704918032],
            [0.0, 0.8406311475409833, 3.812862704918032],
        ),
        (
            VehicleKind::Humvee,
            [0.0, 0.9584999933682383, 0.01044000171124937],
            [2.0988000003419818, 2.348999987514317, 4.510080004044771],
            [0.0, 1.15, 1.53],
            [0.0, 1.917, 1.53],
        ),
    ];
    for (kind, center, size, muzzle, visual_muzzle) in expected {
        let dims = tank_dimensions(kind);
        let close = |actual: DVec3, expected: [f64; 3], what: &str| {
            assert!(
                (actual - DVec3::from_array(expected)).abs().max_element() < 1e-6,
                "{kind:?} {what}: {actual} vs {expected:?}"
            );
        };
        close(dims.center, center, "center");
        close(dims.size, size, "size");
        close(dims.muzzle, muzzle, "muzzle");
        close(dims.visual_muzzle, visual_muzzle, "visual muzzle");
    }
}

/// Every vehicle, either team and with its turret ring open, keeps the joints
/// presentation animates and stays within a part and triangle budget (about 1.5
/// times what it draws today): up to 30 vehicles share a Scrap Yard frame, its
/// shadow pass and the harbor's reflection.
#[test]
fn vehicles_stay_within_their_part_and_triangle_budgets() {
    // (kind, drawn parts, triangles); the Humvee's matches its own budget test.
    let budgets = [
        (VehicleKind::Scout, 195, 10_500),
        (VehicleKind::Balanced, 225, 11_700),
        (VehicleKind::Heavy, 205, 11_900),
        (VehicleKind::Humvee, 47, 15_000),
    ];
    // Draws of one wreck piece: a batch per material.
    const WRECK_BATCHES: usize = 10;
    for (kind, parts, triangles) in budgets {
        for (team, open) in [(Team::Blue, false), (Team::Red, false), (Team::Blue, true)] {
            let model = tank_model_variant(kind, team, open);
            assert_eq!(model.name, kind.as_str());
            for joint in [
                part::HULL,
                part::TURRET,
                part::BARREL,
                part::TRACK_GROUP,
                part::MUZZLE,
            ] {
                assert!(model.find(joint).is_some(), "{kind:?} has no {joint}");
            }
            let drawn = counts(&model);
            assert!(
                drawn.0 <= parts && drawn.1 <= triangles,
                "{kind:?} {team:?} open ring {open}: {drawn:?} (parts, triangles)"
            );
        }
        // Wrecks merge their parts per material and bake the paint into vertex
        // colours.
        for wreck_part in [
            WreckPart::Intact,
            WreckPart::Hull,
            WreckPart::Turret,
            WreckPart::TurretBarrel,
            WreckPart::Barrel,
        ] {
            let wreck = wreck_model(kind, Team::Red, wreck_part);
            let batches = wreck.children.len();
            assert!(
                (1..=WRECK_BATCHES).contains(&batches),
                "{kind:?} {wreck_part:?}: {batches} batches"
            );
            for batch in &wreck.children {
                let drawable = batch.drawable.as_ref().expect("a wreck batch");
                assert!(drawable.material.vertex_colors, "{kind:?} {wreck_part:?}");
            }
        }
    }
}
#[test]
fn painted_parts_wear_the_livery() {
    let livery = |team| {
        let model = tank_model(VehicleKind::Balanced, team);
        let hull = model.find(part::HULL).unwrap();
        hull.children[1].drawable.clone().unwrap()
    };
    let (blue, red) = (livery(Team::Blue), livery(Team::Red));
    // Each team's armor wears one white, vertex-colored paint material whose
    // effect paints the team color and rims it in the team hue, with the wear tile
    // as its effect texture; the vertices carry edge data and the paint value.
    for (team, drawable) in [(Team::Blue, &blue), (Team::Red, &red)] {
        let armor = &drawable.material;
        assert!(armor.vertex_colors);
        assert_eq!(armor.color.0, 0xffffff);
        assert!(armor.map.is_none());
        assert_eq!(
            armor.extra_textures[0].1.source,
            TextureSource::File(tank_surfaces::ARMOR_PAINT_TEXTURE)
        );
        let crate::scene::Effect::Custom { name, params } = &armor.effect else {
            panic!("{team:?} armor has no paint effect");
        };
        assert_eq!(*name, VEHICLE_PAINT_EFFECT);
        // The paint and its rim follow the team's hue.
        let paint_is_blue = params[2] > params[0];
        let rim_is_blue = params[10] > params[8];
        assert_eq!(paint_is_blue, team == Team::Blue);
        assert_eq!(rim_is_blue, team == Team::Blue);
        assert!(armor.tone_mapped);
        assert!(tank_surfaces::is_camouflaged(armor));
    }
    // The trim (lower hull, first) shares the team paint's material, and its
    // value codes sit below the paint's.
    let model = tank_model(VehicleKind::Balanced, Team::Red);
    let hull = model.find(part::HULL).unwrap();
    let trim = hull.children[0].drawable.as_ref().unwrap();
    assert!(Arc::ptr_eq(&trim.material, &red.material));
    let codes = |mesh: &crate::geometry::Mesh| -> (f32, f32) {
        mesh.colors
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), c| {
                (low.min(c[2]), high.max(c[2]))
            })
    };
    assert!(codes(&trim.mesh).1 < 0.5, "{:?}", codes(&trim.mesh));
    assert!(codes(&red.mesh).0 >= 0.5, "{:?}", codes(&red.mesh));
    // Plates carry edge data: some corners lie on a convex edge (state 1 near or 3
    // far) and some channels have no border at all.
    let states: std::collections::HashSet<i32> = red
        .mesh
        .colors
        .iter()
        .flat_map(|c| [c[0], c[1]])
        .map(|channel| channel.floor() as i32)
        .collect();
    assert!(states.contains(&1) || states.contains(&3), "{states:?}");
    // A wreck gets plain paint colors back.
    let mut burnt = (*red.mesh).clone();
    tank_surfaces::unpaint(&mut burnt, &red.material);
    assert!(burnt.colors.iter().all(|c| c[0] > c[1] && c[0] > c[2]));
    // The running gear keeps its own vertex-colored materials without the paint.
    let model = tank_model(VehicleKind::Balanced, Team::Red);
    let hull = model.find(part::HULL).unwrap();
    let gear: Vec<_> = hull
        .children
        .iter()
        .filter_map(|child| child.drawable.as_ref())
        .filter(|drawable| drawable.material.vertex_colors && drawable.material.map.is_none())
        .collect();
    assert!(gear.len() > 40);
}
