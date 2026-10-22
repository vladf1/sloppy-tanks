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
        (VehicleKind::Scout, 0, 215, 25428, 8502, 168, 12, 6, 28),
        (VehicleKind::Scout, 1, 215, 25518, 8532, 168, 12, 6, 28),
        (VehicleKind::Balanced, 0, 238, 27336, 9138, 184, 12, 7, 34),
        (VehicleKind::Balanced, 1, 238, 27378, 9152, 184, 12, 7, 34),
        (VehicleKind::Heavy, 0, 225, 28164, 9414, 171, 12, 7, 34),
        (VehicleKind::Heavy, 1, 225, 28122, 9400, 171, 12, 7, 34),
        (VehicleKind::Humvee, 0, 31, 32736, 10938, 11, 6, 4, 4),
        (VehicleKind::Humvee, 1, 31, 32736, 10938, 11, 6, 4, 4),
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
        (VehicleKind::Scout, (218, 25942, 8720)),
        (VehicleKind::Balanced, (241, 27850, 9356)),
        (VehicleKind::Heavy, (228, 28678, 9632)),
        (VehicleKind::Humvee, (31, 32736, 10938)),
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
            (3666, 0x4f050984, 3075.6851, 0xf0797821, 0x98cf1b48),
            (3144, 0xe8dda8dd, 1211.0354, 0x71d16171, 0x61540731),
            (666, 0x985c2eba, 279.3541, 0xaded77ac, 0x8a737151),
            (108, 0xc7a113b5, 38.8671, 0x3ddbc8a5, 0x9e97af5e),
            (288, 0xd47a0c0d, -11.9209, 0x94d4f49d, 0xabe5a09e),
            (10104, 0xf90a20cb, -3493.5684, 0x5fc0278d, 0x4c16684c),
        ]),
        (VehicleKind::Humvee, 0, WreckPart::TurretBarrel, &[
            (4560, 0x7b5fde4a, 1209.6605, 0x423aa6fe, 0xb25212fe),
            (468, 0x74e31910, -19.0127, 0x96734335, 0x3079eafc),
            (936, 0xb40fca71, -27.4347, 0xe9946e96, 0xbc244bd7),
            (348, 0x4b30ac35, -7.1422, 0xa2ea73b1, 0x1593f5ea),
        ]),
        (VehicleKind::Scout, 0, WreckPart::Intact, &[
            (7422, 0x6d96c8f4, 4823.1074, 0xce91e238, 0xe58a8e63),
            (5460, 0x9a6106f9, 390.1139, 0x1e5e57fb, 0xfefb55d0),
            (2100, 0xf828a961, 3204.6101, 0x9a0b24dd, 0x720d5792),
            (468, 0xc039e97b, -37.2676, 0x6ee76939, 0x8e5c3fb4),
            (9348, 0x0553502c, -3891.0917, 0x12f05d4d, 0x31a37b97),
            (384, 0x102d4537, 3.9914, 0x7f59b8e5, 0xb162036b),
        ]),
        (VehicleKind::Balanced, 1, WreckPart::Barrel, &[
            (480, 0x83d3ce01, -1.2731, 0x11ddcd3c, 0xe3f44d2e),
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
