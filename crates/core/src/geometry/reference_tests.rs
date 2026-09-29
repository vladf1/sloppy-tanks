//! Generated meshes compared with the Three.js r185 generators. Expected values were
//! printed by running the same Three.js constructors and operations in Node: vertex
//! and index counts, and FNV-1a hashes over the f32 bit patterns of every position,
//! normal and UV (and over every index), so any single differing bit fails. A few
//! plain sample values keep failures readable.

use std::f64::consts::PI;

use glam::{DVec2, DVec3};

use super::math::{compose, quat_from_euler, scale_hex_color};
use super::*;

fn fnv(words: impl IntoIterator<Item = u32>) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for word in words {
        h ^= word;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn float_hash<const N: usize>(values: &[[f32; N]]) -> Option<u32> {
    (!values.is_empty()).then(|| fnv(values.iter().flatten().map(|v| v.to_bits())))
}

/// (vertices, indices, positions hash, normals hash, uvs hash, index hash).
type Summary = (
    usize,
    Option<usize>,
    u32,
    Option<u32>,
    Option<u32>,
    Option<u32>,
);

fn summary(mesh: &Mesh) -> Summary {
    (
        mesh.vertex_count(),
        mesh.indices.as_ref().map(Vec::len),
        float_hash(&mesh.positions).unwrap_or(0),
        float_hash(&mesh.normals),
        float_hash(&mesh.uvs),
        mesh.indices.as_ref().map(|i| fnv(i.iter().copied())),
    )
}

fn points(values: &[[f64; 2]]) -> Vec<DVec2> {
    values.iter().map(|p| DVec2::new(p[0], p[1])).collect()
}

fn belt_shape() -> Shape {
    let mut path = Path::new();
    path.move_to(-0.9, -0.33)
        .line_to(0.9, -0.33)
        .absarc(0.9, 0.0, 0.33, -PI / 2.0, PI / 2.0, false)
        .line_to(-0.9, 0.33)
        .absarc(-0.9, 0.0, 0.33, PI / 2.0, PI * 1.5, false);
    Shape::new(path)
}

fn holed_shape() -> Shape {
    let mut path = Path::new();
    path.move_to(0.0, 0.0)
        .line_to(2.0, 0.0)
        .quadratic_curve_to(2.6, 1.0, 2.0, 2.0)
        .bezier_curve_to(DVec2::new(1.5, 2.5), DVec2::new(0.5, 2.5), 0.0, 2.0)
        .close_path();
    let mut hole = Path::new();
    hole.absarc(1.0, 1.0, 0.4, 0.0, PI * 2.0, true);
    Shape {
        outline: path,
        holes: vec![hole],
    }
}

fn tire_profile() -> Vec<DVec2> {
    points(&[
        [0.235, -0.14],
        [0.36, -0.16],
        [0.46, -0.12],
        [0.48, -0.065],
        [0.48, 0.065],
        [0.46, 0.12],
        [0.36, 0.16],
        [0.235, 0.14],
    ])
}

fn tapered_cylinder() -> Mesh {
    CylinderGeometry {
        radius_top: 0.2,
        radius_bottom: 0.5,
        height: 1.5,
        radial_segments: 7,
        height_segments: 3,
        ..CylinderGeometry::default()
    }
    .build()
}

fn extrude_belt() -> Mesh {
    let options = ExtrudeOptions {
        depth: 0.54,
        bevel_enabled: false,
        curve_segments: 6,
        ..ExtrudeOptions::default()
    };
    let mut mesh = extrude_geometry(&[belt_shape()], &options);
    mesh.translate(0.0, 0.0, -0.27).rotate_y(PI / 2.0);
    mesh
}

fn generated() -> Vec<(&'static str, Mesh)> {
    let mut meshes = vec![
        (
            "box",
            BoxGeometry {
                width: 1.2,
                height: 0.5,
                depth: 2.0,
                width_segments: 2,
                height_segments: 1,
                depth_segments: 3,
            }
            .build(),
        ),
        (
            "rounded_box_a",
            rounded_box_geometry(1.94, 0.1, 1.84, 1, 0.02),
        ),
        (
            "rounded_box_b",
            rounded_box_geometry(0.7, 1.3, 0.4, 2, 0.06),
        ),
        ("plane_rotated", {
            let mut mesh = plane_geometry(1.0, 1.0);
            mesh.rotate_x(PI / 2.0);
            mesh
        }),
        ("plane_grid", plane_geometry_segments(2.0, 3.0, 2, 3)),
        ("circle", circle_geometry(0.075, 16)),
        (
            "ring_arc",
            RingGeometry {
                inner_radius: 0.89,
                outer_radius: 1.02,
                theta_segments: 48,
                phi_segments: 1,
                theta_start: PI / 2.0,
                ..RingGeometry::default()
            }
            .build(),
        ),
        ("ring_rotated", {
            let mut mesh = ring_geometry(0.5, 0.65, 24);
            mesh.rotate_x(-PI / 2.0);
            mesh
        }),
        ("cylinder", cylinder_geometry(0.34, 0.34, 0.055, 10)),
        (
            "cylinder_open",
            CylinderGeometry {
                radius_top: 0.65,
                radius_bottom: 0.65,
                height: 0.3,
                radial_segments: 24,
                open_ended: true,
                ..CylinderGeometry::default()
            }
            .build(),
        ),
        ("cylinder_tapered", tapered_cylinder()),
        ("cone", {
            let mut mesh = cone_geometry(0.1, 0.5, 10);
            mesh.rotate_x(PI / 2.0).translate(0.0, 0.0, 0.3);
            mesh
        }),
        ("sphere", sphere_geometry(0.14, 8, 6)),
        (
            "torus_arc",
            TorusGeometry {
                radius: 0.105,
                tube: 0.022,
                radial_segments: 6,
                tubular_segments: 12,
                arc: PI,
                ..TorusGeometry::default()
            }
            .build(),
        ),
        ("icosahedron", icosahedron_geometry(1.0, 0)),
        ("icosahedron_detail", icosahedron_geometry(0.18, 1)),
        ("octahedron", octahedron_geometry(1.0, 0)),
        ("tetrahedron", tetrahedron_geometry(0.75, 0)),
        ("lathe", {
            let mut mesh = lathe_geometry(&tire_profile(), 20, 0.0, PI * 2.0);
            mesh.rotate_z(PI / 2.0);
            mesh
        }),
        (
            "shape_arrow",
            shape_geometry(
                &[Shape::from_points(&points(&[
                    [-0.28, -0.55],
                    [0.28, 0.0],
                    [-0.28, 0.55],
                    [-0.48, 0.37],
                    [-0.1, 0.0],
                    [-0.48, -0.37],
                ]))],
                12,
            ),
        ),
        ("extrude_belt", extrude_belt()),
    ];
    let mut profile = Path::from_points(&points(&[
        [-2.02, 0.78],
        [1.06, 0.78],
        [0.74, 1.64],
        [-1.02, 1.64],
        [-2.02, 1.08],
    ]));
    profile.close_path();
    let profile_options = ExtrudeOptions {
        depth: 1.82,
        bevel_enabled: false,
        ..ExtrudeOptions::default()
    };
    meshes.push((
        "extrude_profile",
        extrude_geometry(&[Shape::new(profile)], &profile_options),
    ));
    let bevel_options = ExtrudeOptions {
        depth: 0.5,
        steps: 2,
        curve_segments: 5,
        ..ExtrudeOptions::default()
    };
    meshes.push((
        "extrude_bevel_hole",
        extrude_geometry(&[holed_shape()], &bevel_options),
    ));
    meshes.push(("shape_hole", shape_geometry(&[holed_shape()], 5)));

    let mut recomputed = tapered_cylinder();
    recomputed.scale(1.0, 2.0, 0.5);
    recomputed.compute_vertex_normals();
    meshes.push(("cylinder_recomputed", recomputed));
    meshes.push((
        "creased",
        icosahedron_geometry(1.0, 1).to_creased_normals(PI / 3.0),
    ));
    let mut cylinder = cylinder_geometry(0.3, 0.3, 1.0, 6);
    cylinder.translate(1.0, 0.0, 0.0);
    let merged = merge_geometries(&[&box_geometry(1.0, 2.0, 3.0), &cylinder]);
    meshes.push(("merged", merged.expect("both meshes are indexed")));
    let mut transformed = sphere_geometry(1.0, 6, 4).to_non_indexed();
    transformed.apply_matrix4(&compose(
        DVec3::new(1.0, 2.0, 3.0),
        quat_from_euler(0.3, -0.7, 1.1),
        DVec3::new(2.0, 0.5, 1.5),
    ));
    meshes.push(("sphere_transformed", transformed));
    let mut centered = box_geometry(1.0, 2.0, 3.0);
    centered.translate(3.0, -1.0, 0.5).center();
    meshes.push(("centered", centered));
    meshes
}

#[rustfmt::skip]
const EXPECTED: &[(&str, Summary)] = &[
    ("box", (52, Some(132), 0xbbd6d4f1, Some(0x75b3f3f5), Some(0x35c4c015), Some(0xc8b96999))),
    ("rounded_box_a", (324, None, 0xb0d21d7d, Some(0x04279eb5), Some(0xd5051a9d), None)),
    ("rounded_box_b", (900, None, 0x0788dd6d, Some(0xb5ad8085), Some(0x9b5eca15), None)),
    ("plane_rotated", (4, Some(6), 0x05bf4685, Some(0x3f262745), Some(0xd6e17165), Some(0x9536c018))),
    ("plane_grid", (12, Some(36), 0x58622395, Some(0x58e22395), Some(0x8559c6ed), Some(0x821b8c27))),
    ("circle", (18, Some(48), 0xadbd2ad8, Some(0x2557db5d), Some(0x3eb1adb5), Some(0x38207cf5))),
    ("ring_arc", (98, Some(288), 0xa60086e2, Some(0x9ef52e1d), Some(0x8fa00c97), Some(0x60d00685))),
    ("ring_rotated", (50, Some(144), 0x7f4ee07b, Some(0xff9f2685), Some(0x9c590c22), Some(0x45cb6f95))),
    ("cylinder", (64, Some(120), 0xcea304b5, Some(0xa5b77e15), Some(0x23399bc1), Some(0xd58a4bcb))),
    ("cylinder_open", (50, Some(144), 0xb8bc2085, Some(0xf3f0378d), Some(0xb897aad5), Some(0x45cb6f95))),
    ("cylinder_tapered", (62, Some(168), 0x23a509cc, Some(0xaa0e4b4d), Some(0x6858f589), Some(0x2edfc769))),
    ("cone", (43, Some(60), 0x7110c36e, Some(0xacf8a0ef), Some(0x3a139009), Some(0x4ff26265))),
    ("sphere", (63, Some(240), 0x18a6fc6b, Some(0x2b3617a5), Some(0x2e01b933), Some(0x787f0375))),
    ("torus_arc", (91, Some(432), 0xc063de92, Some(0x9ba4e97f), Some(0x067fae25), Some(0x86356b3d))),
    ("icosahedron", (60, None, 0xa8ed5e15, Some(0xb5c86621), Some(0x12b171aa), None)),
    ("icosahedron_detail", (240, None, 0x75f1b8f1, Some(0x388a4419), Some(0xc36c3222), None)),
    ("octahedron", (24, None, 0x8fefde65, Some(0xea4d8f85), Some(0x3b55ff85), None)),
    ("tetrahedron", (12, None, 0x66a29961, Some(0x88f2bda5), Some(0x0acc9465), None)),
    ("lathe", (168, Some(840), 0x9359eb28, Some(0xca0b109c), Some(0x86196044), Some(0x78a8a4b5))),
    ("shape_arrow", (6, Some(12), 0x8edf170f, Some(0x9011a50d), Some(0x04f00531), Some(0x40850553))),
    ("extrude_belt", (300, None, 0xf2134e41, Some(0xb5ceeaa5), Some(0xcce05719), None)),
    ("extrude_profile", (48, None, 0x615c8ddd, Some(0x15dd20b3), Some(0x0eb37e1a), None)),
    ("extrude_bevel_hole", (1188, None, 0xba989d95, Some(0xf9ac4bca), Some(0x48172e0e), None)),
    ("shape_hole", (22, Some(66), 0x44c24adb, Some(0x5ca5accd), Some(0xa5b1cf0b), Some(0x5ea98927))),
    ("cylinder_recomputed", (62, Some(168), 0x25a509cc, Some(0xd708301b), Some(0x6858f589), Some(0x2edfc769))),
    ("creased", (240, None, 0x671ea39d, Some(0xb010222e), Some(0xc36c3222), None)),
    ("merged", (64, Some(108), 0xa64be8d5, Some(0xd6158f7d), Some(0x396c01d5), Some(0xca77ed43))),
    ("sphere_transformed", (108, None, 0xec566db8, Some(0x694ab5f3), Some(0x276afc75), None)),
    ("centered", (24, Some(36), 0xdfefde65, Some(0x79efde65), Some(0x9a55ff85), Some(0xd23ef709))),
];

#[test]
fn generated_meshes_match_three_js_bit_for_bit() {
    let meshes = generated();
    assert_eq!(meshes.len(), EXPECTED.len());
    for ((name, mesh), (expected_name, expected)) in meshes.iter().zip(EXPECTED) {
        assert_eq!(name, expected_name);
        assert_eq!(summary(mesh), *expected, "{name}");
    }
}

#[test]
fn sample_values_match_three_js() {
    let meshes = generated();
    let mesh = |name: &str| &meshes.iter().find(|(n, _)| *n == name).unwrap().1;
    let wide = |v: [f64; 3]| v.map(|x| x as f32);

    let boxed = mesh("box");
    assert_eq!(boxed.positions[0], [0.6, 0.25, 1.0]);
    assert_eq!(&boxed.indices.as_ref().unwrap()[..6], &[0, 4, 1, 4, 5, 1]);

    let lathe = mesh("lathe");
    assert_eq!(
        lathe.positions[0],
        wide([
            0.14000000059604645,
            -8.57252763722393e-18,
            0.23499999940395355
        ])
    );
    assert_eq!(
        lathe.normals[84],
        wide([
            -0.10748184472322464,
            1.2833660881645921e-16,
            -0.9942070245742798
        ])
    );
    assert_eq!(&lathe.indices.as_ref().unwrap()[..6], &[0, 8, 1, 9, 1, 8]);

    let belt = mesh("extrude_belt");
    assert_eq!(
        belt.positions[299],
        wide([
            0.27000001072883606,
            -0.31875553727149963,
            0.9854102730751038
        ])
    );
    assert_eq!(belt.uvs[299], [-0.985_410_3, 0.46]);

    let ico = mesh("icosahedron_detail");
    assert_eq!(ico.uvs[120], [0.941_930_1, 0.333_333_34]);
    assert_eq!(ico.normals[120], [0.809_017, -0.5, 0.309_017]);
}

#[test]
fn triangulation_matches_three_js() {
    // Tank armor roof: a tapered chamfered outline with a 24-point turret ring hole.
    let outline = [
        [-0.38, -0.5],
        [0.38, -0.5],
        [0.5, -0.36],
        [0.5, 0.32],
        [0.32, 0.5],
        [-0.32, 0.5],
        [-0.5, 0.32],
        [-0.5, -0.36],
    ];
    let (taper, width, depth, opening) = (0.86, 2.0, 5.244, 0.76);
    let mut contour: Vec<DVec2> = outline
        .iter()
        .map(|[x, z]| DVec2::new(x * taper, z * taper - 0.04))
        .collect();
    let hole = (0..24)
        .map(|i| {
            let angle = (f64::from(i) / 24.0) * PI * 2.0;
            DVec2::new(
                (angle.cos() * opening) / width,
                (angle.sin() * opening - 0.12) / depth,
            )
        })
        .collect();
    let faces: Vec<u32> = triangulate_shape(&mut contour, &mut [hole])
        .iter()
        .flatten()
        .map(|&i| i as u32)
        .collect();
    assert_eq!(faces.len(), 96);
    assert_eq!(&faces[..9], &[20, 7, 0, 0, 1, 2, 3, 4, 5]);
    assert_eq!(fnv(faces.iter().copied()), 0xed4e5232);

    // More than 80 vertices takes earcut's z-order hashed path.
    let mut star: Vec<DVec2> = (0..90)
        .map(|i| {
            let angle = (f64::from(i) / 90.0) * PI * 2.0;
            let r = if i % 2 == 1 {
                1.0
            } else {
                0.6 + 0.1 * (f64::from(i) * 1.7).sin()
            };
            DVec2::new(angle.cos() * r, angle.sin() * r)
        })
        .collect();
    let faces: Vec<u32> = triangulate_shape(&mut star, &mut [])
        .iter()
        .flatten()
        .map(|&i| i as u32)
        .collect();
    assert_eq!(faces.len(), 264);
    assert_eq!(&faces[..9], &[88, 89, 0, 0, 1, 2, 2, 3, 4]);
    assert_eq!(fnv(faces.iter().copied()), 0xd5283aa8);
}

#[test]
fn catmull_rom_matches_three_js() {
    let creek = CatmullRomCurve3::new(
        [
            [150.0, -107.0],
            [85.0, -86.0],
            [15.0, -82.0],
            [-57.0, -85.0],
            [-81.0, -64.0],
            [-81.0, -12.0],
            [-86.0, 48.0],
            [-110.0, 145.0],
        ]
        .iter()
        .map(|[x, z]| DVec3::new(*x, -2.65, *z))
        .collect(),
    );
    let close = |a: DVec3, b: [f64; 3]| (a - DVec3::from_array(b)).length() < 1e-9;
    let samples = creek.points(160);
    assert_eq!(samples.len(), 161);
    assert!(close(
        samples[37],
        [41.949101284654, -2.65, -82.23866316183937]
    ));
    assert!(close(samples[160], [-109.99999999999999, -2.65, 145.0]));
    assert!(close(
        creek.tangent(37.0 / 160.0),
        [-0.9997837668896065, 0.0, 0.020794697978794987]
    ));
    assert!((creek.length() - 456.48016783428596).abs() < 1e-9);
    assert!(close(
        creek.point_at(0.3),
        [16.651632056626287, -2.65, -82.01183464017014]
    ));
}

#[test]
fn shade_colors_match_three_js() {
    // `new THREE.Color(team).multiplyScalar(0.62).getHex()` for both team colors.
    assert_eq!(scale_hex_color(0x008cff, 0.62), 28878);
    assert_eq!(scale_hex_color(0xff303e, 0.62), 13509936);
}
