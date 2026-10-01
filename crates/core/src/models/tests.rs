//! Vehicle models compared with the TypeScript/Three.js implementation. Expected
//! values were printed by running `tankModel`, `tankHull`/`tankVisualMuzzle` and
//! `wreckModel` (with the browser's painted materials) in Node.

use glam::DVec3;

use super::*;
use crate::scene::Node;

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
fn vehicle_models_match_typescript_counts() {
    // (kind, team, meshes, vertices, triangles, hull, turret, barrel, track-group children)
    let expected = [
        (VehicleKind::Scout, 0, 161, 13808, 5396, 115, 9, 3, 36),
        (VehicleKind::Scout, 1, 161, 13844, 5408, 115, 9, 3, 36),
        (VehicleKind::Balanced, 0, 164, 14064, 5540, 117, 9, 4, 36),
        (VehicleKind::Balanced, 1, 164, 14100, 5552, 117, 9, 4, 36),
        (VehicleKind::Heavy, 0, 163, 14732, 5712, 116, 9, 4, 36),
        (VehicleKind::Heavy, 1, 163, 14768, 5724, 116, 9, 4, 36),
        (VehicleKind::Humvee, 0, 30, 43644, 14548, 11, 6, 4, 4),
        (VehicleKind::Humvee, 1, 30, 43644, 14548, 11, 6, 4, 4),
    ];
    for (kind, team, meshes, vertices, triangles, hull, turret, barrel, tracks) in expected {
        let model = tank_model(kind, Team::from_index(team));
        assert_eq!(model.name, kind.name());
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
        (VehicleKind::Scout, (164, 14134, 5614)),
        (VehicleKind::Balanced, (167, 14390, 5758)),
        (VehicleKind::Heavy, (166, 15058, 5930)),
        (VehicleKind::Humvee, (30, 43644, 14548)),
    ];
    for (kind, expected) in open {
        let model = tank_model_variant(kind, Team::Blue, false, true);
        assert_eq!(counts(&model), expected, "{kind:?} with open turret ring");
    }
}

#[test]
fn painted_parts_use_the_wear_texture() {
    let model = tank_model(VehicleKind::Balanced, Team::Red);
    let hull = model.find(part::HULL).unwrap();
    let armor = hull.children[1].drawable.as_ref().unwrap();
    // Service paint, not the glowing team color: lit and tone mapped like the scene.
    assert_eq!(armor.material.color.0, tank_surfaces::VEHICLE_PAINT[1]);
    assert_eq!(armor.material.map, Some(armor_wear_texture()));
    assert_eq!(armor.material.emissive.0, 0);
    assert!(armor.material.tone_mapped);
    // Dark belts keep plain paint.
    let belts: Vec<_> = hull
        .children
        .iter()
        .filter_map(|child| child.drawable.as_ref())
        .filter(|drawable| drawable.mesh.vertex_count() == 300)
        .collect();
    assert_eq!(belts.len(), 2);
    assert!(belts.iter().all(|belt| belt.material.map.is_none()));
}

fn fnv(words: impl IntoIterator<Item = u32>) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for word in words {
        h ^= word;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn float_hash<const N: usize>(values: &[[f32; N]]) -> u32 {
    fnv(values.iter().flatten().map(|v| v.to_bits()))
}

/// Per batched mesh: vertex count and position, normal, UV and color hashes.
type BatchSummary = (usize, u32, u32, u32, u32);

#[test]
fn wreck_batches_match_typescript() {
    #[rustfmt::skip]
    let expected: [(VehicleKind, usize, WreckPart, &[BatchSummary]); 4] = [
        (VehicleKind::Heavy, 1, WreckPart::Hull, &[
            (1302, 0x3a8a211f, 0xa9a70f0e, 0x70779971, 0x79bae59b),
            (5184, 0x12691405, 0x3ae44cf5, 0x007bd521, 0x963c26c5),
            (1044, 0xa26f2e7e, 0xdfd6549d, 0xe2a6ad73, 0xb9ae5fe1),
            (72, 0xe4723595, 0xf5ae0c53, 0x35c99581, 0xfffc251d),
            (108, 0xc7a113b5, 0xc1e1cb55, 0x3ddbc8a5, 0xc9566961),
            (72, 0x89e75edd, 0x731afb65, 0x7938d925, 0x0c45cab5),
            (144, 0x44a5e11e, 0x637e9bc5, 0xa2def1b1, 0x5f692175),
            (144, 0x5ca3de75, 0x162a9a1f, 0x951bbe05, 0x16256455),
            (288, 0xd47a0c0d, 0x56200137, 0x94d4f49d, 0xa2f91ec5),
            (1440, 0xca1cd9b5, 0x640e8855, 0x9d04ab85, 0xd41fae65),
            (432, 0xd86addd1, 0xfaac1d05, 0x2050cd45, 0x5374f895),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (4560, 0x7b5fde4a, 0x1e53e955, 0x84d726eb, 0x8efa2f55),
            (468, 0x74e31910, 0xdad78ac5, 0x96734335, 0x80f71295),
            (936, 0xb40fca71, 0xd159cf07, 0xe9946e96, 0x9aef1fdd),
            (348, 0x4b30ac35, 0xe40decbb, 0xa2ea73b1, 0x25856219),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (4260, 0xc974c3c3, 0x455307df, 0x875d170f, 0x12d74ca9),
            (6492, 0x589c1bbe, 0xd69ba0ab, 0x02a2555c, 0x0005f0f5),
            (1872, 0xd431b9d5, 0xd6a3c5e7, 0x14c97d70, 0x38dac18d),
            (564, 0x4bb34e4d, 0x691baff3, 0xdd844d2d, 0x003fa771),
            (468, 0xd7e1d79b, 0xf492c575, 0x14350951, 0x8715f7c9),
            (60, 0xf347d68e, 0x9525485d, 0x1ef2798c, 0xf543efd9),
            (72, 0xf81a202d, 0x8f9161e5, 0xa3ee9e1d, 0x0c45cab5),
            (1440, 0x9ef6793d, 0x640e8855, 0x9d04ab85, 0x82d04ba5),
            (360, 0xaf7f8b41, 0xc3081b25, 0xee11a805, 0xd544458d),
            (456, 0xcbf8aaa7, 0x290c9454, 0x5ff550b2, 0x701aa04d),
            (144, 0x83237b42, 0xa782a24b, 0x6d704989, 0x924eb995),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (480, 0x9816bc91, 0x918e3fbb, 0x3dd99165, 0x88b38325),
            (816, 0xb6e9fb17, 0xb7f6ddc8, 0x1a284e7b, 0xff88b805),
            (36, 0xd1ad26bd, 0x347a9e35, 0x13c61651, 0x022a75b9),
            (144, 0x4c0b62a7, 0xa782a24b, 0x6d704989, 0x924eb995),
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
                    float_hash(&mesh.normals),
                    float_hash(&mesh.uvs),
                    float_hash(&mesh.colors),
                )
            })
            .collect();
        assert_eq!(actual, batches, "{kind:?}/{team}/{wreck_part:?}");
    }
}
