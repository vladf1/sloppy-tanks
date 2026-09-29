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
        (VehicleKind::Scout, 0, 195, 6414, 3304, 130, 22, 9, 36),
        (VehicleKind::Scout, 1, 196, 6438, 3316, 130, 23, 9, 36),
        (VehicleKind::Balanced, 0, 201, 6734, 3500, 132, 31, 4, 36),
        (VehicleKind::Balanced, 1, 202, 6758, 3512, 132, 32, 4, 36),
        (VehicleKind::Heavy, 0, 205, 6654, 3424, 138, 29, 4, 36),
        (VehicleKind::Heavy, 1, 206, 6678, 3436, 138, 30, 4, 36),
        (VehicleKind::Humvee, 0, 364, 13220, 7276, 141, 9, 5, 212),
        (VehicleKind::Humvee, 1, 364, 13220, 7276, 141, 9, 5, 212),
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
        (VehicleKind::Scout, (198, 6740, 3522)),
        (VehicleKind::Balanced, (204, 7060, 3718)),
        (VehicleKind::Heavy, (208, 6980, 3642)),
        (VehicleKind::Humvee, (364, 13220, 7276)),
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
    assert_eq!(armor.material.color.0, TEAM_COLORS[1]);
    assert_eq!(armor.material.map, Some(armor_wear_texture()));
    assert_eq!(armor.material.emissive_intensity, 0.04);
    assert!(!armor.material.tone_mapped);
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
            (168, 0x07919fd3, 0x539f3c63, 0xd4cf4796, 0xca7383bd),
            (162, 0x02c459a9, 0x982f7ac8, 0x173b500c, 0x5952a13b),
            (144, 0xc83dc956, 0x637e9bc5, 0xa2def1b1, 0x5f692175),
            (144, 0x65e4ec2d, 0x162a9a1f, 0x951bbe05, 0x16256455),
            (288, 0xb74a9aad, 0x56200137, 0x94d4f49d, 0xa2f91ec5),
            (1164, 0x267beaf3, 0x369ad59d, 0x68eb05fd, 0xaff0a1f9),
            (3492, 0xd5a3db61, 0xefe1d855, 0xf29bda65, 0xf8683545),
            (2592, 0x1a22b1dd, 0xeb940885, 0x58fadd85, 0x7a375725),
            (288, 0xfff0c33d, 0x3450cd45, 0x5ae0d2c5, 0xb41f18e5),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (624, 0xc1a17dd3, 0x05fae775, 0xb0a72081, 0xb6991535),
            (684, 0x50a16b3d, 0x35147895, 0xb0146df5, 0xf56e338d),
            (432, 0xfc583479, 0x3d3fb12b, 0x3622ed91, 0x80d06ce5),
            (72, 0x67425a9d, 0xa8a41ba5, 0x155c3305, 0x27a8e1a5),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (336, 0x0ce7fb59, 0x8ee27015, 0x37f89fd6, 0xfcba86b5),
            (168, 0xf5d81a4a, 0x56d59baa, 0x12460adf, 0x808c1425),
            (1416, 0x21e2ba23, 0x7a2f20ad, 0xf484bf5d, 0xff32dea5),
            (3276, 0x48ccf29f, 0x18411815, 0x20dc02a5, 0x7aeb3bf1),
            (3888, 0x23974e90, 0xf6a1e6e7, 0xc364c4f9, 0xb1155445),
            (432, 0x4e6e96b5, 0xfaac1d05, 0x2050cd45, 0x75c09d05),
            (396, 0x839f42b8, 0x83f48e77, 0x5274fdc5, 0xc4592e41),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (432, 0xca48c069, 0x3d3fb12b, 0x3622ed91, 0x3f684765),
            (144, 0x6198b2f4, 0xa782a24b, 0x6d704989, 0x924eb995),
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
