//! Vehicle model checks. The hull bounds and muzzles still match what the
//! TypeScript `tankHull`/`tankVisualMuzzle` measured in Node; the mesh counts and
//! wreck batch hashes are golden values of the current Rust models, regenerated
//! when a vehicle model changes on purpose.

use std::sync::Arc;

use glam::DVec3;

use super::*;
use crate::geometry::reference_tests::fnv;
use crate::scene::{Node, TextureSource};

/// (meshes, vertices, triangles) over every drawable in the tree.
fn counts(node: &Node) -> (usize, usize, usize) {
    let mut totals = (0, 0, 0);
    node.traverse(glam::DMat4::IDENTITY, &mut |part, _| {
        if let Some(drawable) = &part.drawable {
            totals.0 += 1;
            totals.1 += drawable.mesh.vertex_count();
            totals.2 += drawable.mesh.triangle_count();
        }
    });
    totals
}

fn child_count(model: &Node, name: &str) -> usize {
    model.find(name).expect(name).children.len()
}

#[test]
fn tank_dimensions_match_typescript() {
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

#[test]
fn vehicle_model_counts() {
    // (kind, team, meshes, vertices, triangles, hull, turret, barrel, track-group children)
    let expected = [
        (VehicleKind::Scout, 0, 133, 21204, 7094, 110, 12, 6, 0),
        (VehicleKind::Scout, 1, 133, 21294, 7124, 110, 12, 6, 0),
        (VehicleKind::Balanced, 0, 150, 22992, 7690, 126, 12, 7, 0),
        (VehicleKind::Balanced, 1, 150, 23034, 7704, 126, 12, 7, 0),
        (VehicleKind::Heavy, 0, 137, 23736, 7938, 113, 12, 7, 0),
        (VehicleKind::Heavy, 1, 137, 23694, 7924, 113, 12, 7, 0),
        (VehicleKind::Humvee, 0, 32, 32634, 10904, 12, 6, 4, 4),
        (VehicleKind::Humvee, 1, 32, 32634, 10904, 12, 6, 4, 4),
    ];
    for (kind, team, meshes, vertices, triangles, hull, turret, barrel, tracks) in expected {
        let model = tank_model(kind, Team::from_index(team));
        assert_eq!(model.name, kind.as_str());
        assert_eq!(
            counts(&model),
            (meshes, vertices, triangles),
            "{kind:?}/{team}"
        );
        let parts = [
            child_count(&model, part::HULL),
            child_count(&model, part::TURRET),
            child_count(&model, part::BARREL),
            child_count(&model, part::TRACK_GROUP),
        ];
        assert_eq!(parts, [hull, turret, barrel, tracks], "{kind:?}/{team}");
        assert!(model.find(part::MUZZLE).is_some());
    }
    let open = [
        (VehicleKind::Scout, (136, 21718, 7312)),
        (VehicleKind::Balanced, (153, 23506, 7908)),
        (VehicleKind::Heavy, (140, 24250, 8156)),
        (VehicleKind::Humvee, (32, 32634, 10904)),
    ];
    for (kind, expected) in open {
        let model = tank_model_variant(kind, Team::Blue, true);
        assert_eq!(counts(&model), expected, "{kind:?} with open turret ring");
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

fn float_hash<const N: usize>(values: &[[f32; N]]) -> u32 {
    fnv(values.iter().flatten().map(|v| v.to_bits()))
}

/// Normals come from trigonometry whose last bits differ between platforms'
/// math libraries (macOS and glibc), which can flip an `f32` rounding, so they are
/// summarised by a weighted sum that a tolerance compares instead of a bit hash.
fn normal_checksum(normals: &[[f32; 3]]) -> f64 {
    normals
        .iter()
        .enumerate()
        .map(|(i, [x, y, z])| {
            (i % 7 + 1) as f64 * (f64::from(*x) + 2.0 * f64::from(*y) + 3.0 * f64::from(*z))
        })
        .sum()
}
const NORMAL_CHECKSUM_TOLERANCE: f64 = 1e-3;

/// Per batched mesh: vertex count, position hash, normal checksum, UV and color hashes.
type BatchSummary = (usize, u32, f64, u32, u32);

#[test]
fn wreck_batches_match_golden() {
    #[rustfmt::skip]
    let expected: [(VehicleKind, usize, WreckPart, &[BatchSummary]); 4] = [
        (VehicleKind::Heavy, 1, WreckPart::Hull, &[
            (3288, 0xdce2fdb4, 1895.8762, 0xa9da9e3f, 0xf0361396),
            (3522, 0xe910d1f9, 2351.8751, 0x3502a7c6, 0x92381cb9),
            (666, 0x985c2eba, 279.3541, 0xaded77ac, 0x8a737151),
            (108, 0xc7a113b5, 38.8671, 0x3ddbc8a5, 0xc415b1ce),
            (288, 0xd47a0c0d, -11.9209, 0x94d4f49d, 0xabe5a09e),
            (4392, 0x4bdecb59, -17.7343, 0x1ed3cce5, 0x902aa900),
            (1344, 0x3d775bdd, 2.8138, 0xb6ce257d, 0x2add3aad),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (4560, 0x7b5fde4a, 1209.6605, 0x423aa6fe, 0x20c058ba),
            (468, 0x74e31910, -19.0127, 0x96734335, 0x3079eafc),
            (936, 0xb40fca71, -27.4347, 0xe9946e96, 0xbc244bd7),
            (348, 0x4b30ac35, -7.1422, 0xa2ea73b1, 0x1593f5ea),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (5436, 0x019db6bd, 3799.2748, 0xa25cc151, 0x4522bcd3),
            (7206, 0xe1047e7f, 1077.4550, 0x4826094f, 0x0bf2ddef),
            (2268, 0x317b2488, 3230.5660, 0x9d30f92c, 0x605b0e50),
            (468, 0xf62d1b53, -37.2676, 0x2cea9759, 0xa0daa990),
            (3852, 0x85f249fd, 5.7848, 0xc66fb1b5, 0x35b80f2f),
            (1344, 0x546c08d5, 1.5827, 0x10f8f7cd, 0x234753be),
            (384, 0xf38459b3, 3.9914, 0x7f59b8e5, 0x37d5e015),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (480, 0x83d3ce01, -1.2731, 0x11ddcd3c, 0xeee2f49e),
            (936, 0x42d872bf, 290.0387, 0xf6fa33ae, 0xb9eee0c3),
            (300, 0x32eb4057, -4.8805, 0x5fe93231, 0xba882798),
        ]),
    ];
    for (kind, team, wreck_part, batches) in expected {
        let wreck = wreck_model(kind, Team::from_index(team), wreck_part);
        let actual: Vec<_> = wreck
            .children
            .iter()
            .map(|child| {
                let drawable = child.drawable.as_ref().unwrap();
                assert!(drawable.material.vertex_colors);
                let mesh = &drawable.mesh;
                (
                    mesh.vertex_count(),
                    float_hash(&mesh.positions),
                    normal_checksum(&mesh.normals),
                    float_hash(&mesh.uvs),
                    float_hash(&mesh.colors),
                )
            })
            .collect();
        let matches = actual.len() == batches.len()
            && actual.iter().zip(batches).all(|(a, e)| {
                (a.0, a.1, a.3, a.4) == (e.0, e.1, e.3, e.4)
                    && (a.2 - e.2).abs() < NORMAL_CHECKSUM_TOLERANCE
            });
        assert!(
            matches,
            "{kind:?}/{team}/{wreck_part:?}: {actual:?} vs {batches:?}"
        );
    }
}
