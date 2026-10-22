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
        (VehicleKind::Scout, 0, 198, 25806, 8602, 160, 9, 3, 28),
        (VehicleKind::Scout, 1, 198, 25902, 8634, 160, 9, 3, 28),
        (VehicleKind::Balanced, 0, 227, 27234, 9078, 182, 9, 4, 34),
        (VehicleKind::Balanced, 1, 227, 27318, 9106, 182, 9, 4, 34),
        (VehicleKind::Heavy, 0, 214, 27906, 9302, 169, 9, 4, 34),
        (VehicleKind::Heavy, 1, 214, 27954, 9318, 169, 9, 4, 34),
        (VehicleKind::Humvee, 0, 31, 43752, 14584, 12, 6, 4, 4),
        (VehicleKind::Humvee, 1, 31, 43752, 14584, 12, 6, 4, 4),
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
        (VehicleKind::Scout, (201, 26460, 8820)),
        (VehicleKind::Balanced, (230, 27888, 9296)),
        (VehicleKind::Heavy, (217, 28560, 9520)),
        (VehicleKind::Humvee, (31, 43752, 14584)),
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
    // Both teams' armor shares one white, vertex-colored, camouflaged paint
    // material; the team paint lives in the vertex colors.
    assert!(Arc::ptr_eq(&blue.material, &red.material));
    let armor = &red.material;
    assert!(armor.vertex_colors);
    assert_eq!(armor.color.0, 0xffffff);
    assert_eq!(
        armor.map.as_ref().map(|map| map.source.clone()),
        Some(TextureSource::File(tank_surfaces::ARMOR_CAMO_TEXTURE))
    );
    assert_eq!(armor.emissive.0, 0);
    assert!(armor.tone_mapped);
    let mean = |mesh: &crate::geometry::Mesh| {
        let sum = mesh.colors.iter().fold([0.0f64; 3], |acc, c| {
            [0, 1, 2].map(|i| acc[i] + f64::from(c[i]))
        });
        sum.map(|channel| channel / mesh.colors.len() as f64)
    };
    let (blue_paint, red_paint) = (mean(&blue.mesh), mean(&red.mesh));
    assert!(blue_paint[2] > blue_paint[0] * 2.0, "{blue_paint:?}");
    assert!(red_paint[0] > red_paint[2] * 2.0, "{red_paint:?}");
    // Track links keep their own vertex-colored material without the camouflage.
    let model = tank_model(VehicleKind::Balanced, Team::Red);
    let hull = model.find(part::HULL).unwrap();
    let links: Vec<_> = hull
        .children
        .iter()
        .filter_map(|child| child.drawable.as_ref())
        .filter(|drawable| drawable.material.vertex_colors && drawable.material.map.is_none())
        .collect();
    assert!(links.len() > 40);
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
            (7350, 0x35aa053d, 3062.2811, 0xc74a8d91, 0x0f1f4630),
            (1800, 0xc1e59452, 1126.8637, 0x9c89dd39, 0x9a8d495d),
            (876, 0xf6d1a754, 437.4741, 0xc5203d40, 0xdb1ce317),
            (108, 0xc7a113b5, 38.8671, 0x3ddbc8a5, 0x18233382),
            (72, 0x89e75edd, 16.8157, 0x7938d925, 0x24116041),
            (8736, 0x1b037e68, -1430.3482, 0xbc43a5f1, 0x00364355),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (4560, 0x7b5fde4a, 1209.6605, 0xfbc8a72a, 0x5e738242),
            (468, 0x74e31910, -19.0127, 0x96734335, 0x6c1eec23),
            (936, 0xb40fca71, -27.4347, 0xe9946e96, 0x2f48d032),
            (348, 0x4b30ac35, -7.1422, 0xa2ea73b1, 0xe1dde01f),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (11040, 0xa726dbaa, 4715.1059, 0x7d3981bd, 0x786ee1ee),
            (3672, 0xd6cce632, 210.9614, 0x0ab113d8, 0xed53c37a),
            (1992, 0x6e1ec575, 3152.9516, 0x6c284697, 0x863de1c5),
            (468, 0xd7e1d79b, -68.8245, 0x14350951, 0x3cbc40f5),
            (72, 0xf81a202d, 16.9247, 0xa3ee9e1d, 0x62c7b85f),
            (8532, 0x409d8605, -1834.6751, 0x0fd93d1d, 0x78332ba7),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (480, 0x9816bc91, -12.2110, 0xd8c5e96d, 0x19acc2dc),
            (816, 0xb6e9fb17, 617.1525, 0x1a284e7b, 0x7c1a0ef3),
            (180, 0xec2d00af, 9.2942, 0xacf92475, 0x9a75a2a1),
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
