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
        (VehicleKind::Scout, 0, 130, 20776, 6962, 110, 13, 6, 0),
        (VehicleKind::Scout, 1, 130, 20818, 6976, 110, 13, 6, 0),
        (VehicleKind::Balanced, 0, 147, 22666, 7592, 126, 13, 7, 0),
        (VehicleKind::Balanced, 1, 147, 22660, 7590, 126, 13, 7, 0),
        (VehicleKind::Heavy, 0, 134, 23074, 7728, 113, 13, 7, 0),
        (VehicleKind::Heavy, 1, 134, 22984, 7698, 113, 13, 7, 0),
        (VehicleKind::Humvee, 0, 32, 32604, 10894, 12, 6, 4, 4),
        (VehicleKind::Humvee, 1, 32, 32580, 10886, 12, 6, 4, 4),
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
        (VehicleKind::Scout, (133, 21290, 7180)),
        (VehicleKind::Balanced, (150, 23180, 7810)),
        (VehicleKind::Heavy, (137, 23588, 7946)),
        (VehicleKind::Humvee, (32, 32604, 10894)),
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
            (3360, 0x2aa83920, 1915.3177, 0xcf4cdb3f, 0x0fd79720),
            (3480, 0x54245a65, 2356.7810, 0x7927380e, 0x815f7d91),
            (594, 0x1fdb38d4, 274.3541, 0xdee32360, 0xaea680ce),
            (108, 0xc7a113b5, 38.8671, 0x3ddbc8a5, 0xe22e0a49),
            (288, 0xd47a0c0d, -11.9209, 0x94d4f49d, 0xabe5a09e),
            (3864, 0xe480f499, 4.1039, 0xd4c784a5, 0xdd0335d5),
            (1344, 0x3d775bdd, 2.8138, 0xb6ce257d, 0x2add3aad),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (4560, 0x7b5fde4a, 1209.6605, 0x423aa6fe, 0x9a50111d),
            (468, 0x74e31910, -19.0127, 0x96734335, 0x3079eafc),
            (936, 0xb40fca71, -27.4347, 0xe9946e96, 0xc73eb1a6),
            (348, 0x4b30ac35, -7.1422, 0xa2ea73b1, 0x1593f5ea),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (6132, 0x7276c352, 4121.4125, 0x8fdfdc26, 0x5444960b),
            (6792, 0xa7ee2fc8, 1226.3142, 0xb74d69c0, 0x73e18276),
            (1968, 0x5004ce48, 3204.1629, 0xa45c0eac, 0x1954db3f),
            (468, 0xd373f55d, -37.5120, 0x2cea9759, 0x834c4b44),
            (3390, 0x0eae6ddf, 4.6447, 0x7b4ae979, 0x8e9bbaf2),
            (1344, 0x546c08d5, 1.5827, 0x10f8f7cd, 0x234753be),
            (384, 0x12dfa13b, 3.9914, 0x7f59b8e5, 0xdbc379b3),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (768, 0xdcd01919, 6.9842, 0xe6ba195b, 0x1712653d),
            (1020, 0x8934b1b7, 334.7098, 0xcdf32c2c, 0x6e1e4cff),
            (180, 0xec2d00af, 9.2942, 0xacf92475, 0x76736b02),
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
