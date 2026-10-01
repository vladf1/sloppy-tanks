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
        (VehicleKind::Scout, 0, 197, 23392, 7868, 159, 9, 3, 28),
        (VehicleKind::Scout, 1, 197, 23428, 7880, 159, 9, 3, 28),
        (VehicleKind::Balanced, 0, 226, 24556, 8256, 181, 9, 4, 34),
        (VehicleKind::Balanced, 1, 226, 24592, 8268, 181, 9, 4, 34),
        (VehicleKind::Heavy, 0, 213, 25072, 8428, 168, 9, 4, 34),
        (VehicleKind::Heavy, 1, 213, 25108, 8440, 168, 9, 4, 34),
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
        (VehicleKind::Scout, (200, 23718, 8086)),
        (VehicleKind::Balanced, (229, 24882, 8474)),
        (VehicleKind::Heavy, (216, 25398, 8646)),
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
    // Track links (vertex-colored steel and rubber) keep plain, unworn materials.
    let links: Vec<_> = hull
        .children
        .iter()
        .filter_map(|child| child.drawable.as_ref())
        .filter(|drawable| drawable.material.vertex_colors)
        .collect();
    assert!(links.len() > 40);
    assert!(links.iter().all(|link| link.material.map.is_none()));
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
fn wreck_batches_match_typescript() {
    #[rustfmt::skip]
    let expected: [(VehicleKind, usize, WreckPart, &[BatchSummary]); 4] = [
        (VehicleKind::Heavy, 1, WreckPart::Hull, &[
            (1302, 0x3a8a211f, 3144.6345, 0x70779971, 0x8992e097),
            (1584, 0x7905bdd9, -39.7440, 0x6072eca1, 0x7e0f5405),
            (276, 0xff2c0d56, 158.3686, 0x87edbdcb, 0x72c7fce1),
            (72, 0xe4723595, 39.2321, 0x35c99581, 0xfffc251d),
            (108, 0xc7a113b5, 38.8671, 0x3ddbc8a5, 0xc9566961),
            (72, 0x89e75edd, 16.8157, 0x7938d925, 0x0c45cab5),
            (144, 0x44a5e11e, 1140.0000, 0xa2def1b1, 0x5f692175),
            (144, 0x5ca3de75, 17.7027, 0x951bbe05, 0x16256455),
            (288, 0xd47a0c0d, -11.9209, 0x94d4f49d, 0xa2f91ec5),
            (8736, 0x1b037e68, -1430.3482, 0xbc43a5f1, 0x451582ad),
            (4308, 0xd6ace17a, 4.6853, 0x384deb19, 0x5a5a6451),
            (1344, 0x781e9945, -75.5077, 0x314b8499, 0x883731e5),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (4560, 0x7b5fde4a, 1209.6605, 0x84d726eb, 0xdb0237b5),
            (468, 0x74e31910, -19.0127, 0x96734335, 0x80f71295),
            (936, 0xb40fca71, -27.4347, 0xe9946e96, 0x9aef1fdd),
            (348, 0x4b30ac35, -7.1422, 0xa2ea73b1, 0x25856219),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (4260, 0xc974c3c3, 4734.8233, 0x875d170f, 0xa935769d),
            (2892, 0x422d45d6, 107.4076, 0xd9b7c4dc, 0x5bbf7c35),
            (1104, 0xadcd0191, 2345.8737, 0xd074ac58, 0x8afc8a8d),
            (564, 0x4bb34e4d, 0.1359, 0xdd844d2d, 0x003fa771),
            (468, 0xd7e1d79b, -68.8245, 0x14350951, 0x8715f7c9),
            (60, 0xf347d68e, 34.0727, 0x1ef2798c, 0xf543efd9),
            (72, 0xf81a202d, 16.9247, 0xa3ee9e1d, 0x0c45cab5),
            (8532, 0x409d8605, -1834.6751, 0x0fd93d1d, 0x0e491cb5),
            (4308, 0x26bf1eb1, 1.9575, 0xcd8a7719, 0xa568df15),
            (744, 0x6f365b09, 17.1411, 0x5ca996b5, 0x1ceaa305),
            (456, 0xcbf8aaa7, 710.5988, 0x5ff550b2, 0x701aa04d),
            (144, 0x83237b42, 5.1699, 0x6d704989, 0x924eb995),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (480, 0x9816bc91, -12.2110, 0x3dd99165, 0x7bfc68e5),
            (816, 0xb6e9fb17, 617.1525, 0x1a284e7b, 0xff88b805),
            (36, 0xd1ad26bd, 6.0000, 0x13c61651, 0x022a75b9),
            (144, 0x4c0b62a7, 5.1699, 0x6d704989, 0x924eb995),
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
