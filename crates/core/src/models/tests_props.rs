//! Cover, tree and prop models compared with the TypeScript/Three.js implementation.
//!
//! The expected values at the end of this file were printed by a Node script that
//! built `coverModel` for covers of every map layout (village, harbor, quarry,
//! Stress Grid, Scrap Yard) at every damage stage the game shows, synthetic trees
//! of every family, timber walls with hits, rubble seeds, every pickup kind, the
//! flags, the barrel scraps and the trunk fragment. Textures were stubbed to carry
//! their path. Each mesh is summarised by FNV-1a hashes of its attributes, its
//! index, its world matrix, a material descriptor and, for instanced meshes, its
//! f32 instance matrices.
//!
//! `sin`/`cos` of the platform libm and of V8 can differ in the last bit, which
//! moves an occasional f64 matrix element or f32 vertex by one ulp. The asserted
//! hashes therefore read attributes on a 1/65536 grid and matrices on a 2^-20
//! grid; `exact` records the bit-exact hash as well, and the test reports how many
//! covers also match bit for bit.

use glam::DMat4;

use super::*;
use crate::geometry::Mesh;
use crate::geometry::math::{compose, js_round};
use crate::geometry::reference_tests::fnv;
use crate::scene::{Material, Node, TextureSource};
use crate::sim::math::Random;
use crate::sim::timber_layout::{
    TimberHit, TimberJoin, TimberPart, TimberPartKind, TimberWall, timber_parts,
};
use crate::sim::tree_proportions::tree_proportions;
use crate::sim::types::{CoverKind, PickupKind};

/// Whether summaries hash exact bits or grid-rounded values (see the module docs).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Precision {
    Exact,
    Grid,
}

fn value_word(value: f32, precision: Precision) -> u32 {
    match precision {
        Precision::Exact => value.to_bits(),
        Precision::Grid => js_round(f64::from(value) * 65536.0) as i64 as u32,
    }
}

fn values_hash<const N: usize>(values: &[[f32; N]], precision: Precision) -> u32 {
    fnv(values.iter().flatten().map(|&v| value_word(v, precision)))
}

/// The TypeScript `material()` descriptor: color, map path, vertex colors,
/// transparency, side, roughness and metalness.
fn descriptor(material: &Material) -> String {
    let map = match material.map.as_ref().map(|map| &map.source) {
        Some(TextureSource::File(path)) => path,
        _ => "",
    };
    format!(
        "{:x}|{}|{}|{}|{}|{}|{}",
        material.color.0,
        map,
        u8::from(material.vertex_colors),
        u8::from(material.transparent),
        material.side as u8,
        material.roughness,
        material.metalness
    )
}

fn color_hash(mesh: &Mesh, precision: Precision) -> u32 {
    match mesh.vertex_alpha() {
        Some(alpha) => {
            fnv(mesh.colors.iter().zip(alpha).flat_map(|(rgb, a)| {
                [rgb[0], rgb[1], rgb[2], *a].map(|v| value_word(v, precision))
            }))
        }
        None => values_hash(&mesh.colors, precision),
    }
}

fn matrix_hash(matrix: DMat4, precision: Precision) -> u32 {
    fnv(matrix.to_cols_array().iter().flat_map(|v| match precision {
        Precision::Exact => {
            let bits = (v + 0.0).to_bits();
            vec![bits as u32, (bits >> 32) as u32]
        }
        Precision::Grid => vec![js_round(v * 1_048_576.0) as i64 as u32],
    }))
}

fn mesh_hash(node: &Node, world: DMat4, precision: Precision) -> u32 {
    let drawable = node.drawable.as_ref().expect("a mesh");
    let mesh = &drawable.mesh;
    let mut words = vec![
        values_hash(&mesh.positions, precision),
        values_hash(&mesh.normals, precision),
        values_hash(&mesh.uvs, precision),
        color_hash(mesh, precision),
        mesh.indices
            .as_ref()
            .map_or(0, |indices| fnv(indices.iter().copied())),
        matrix_hash(world, precision),
        fnv(descriptor(&drawable.material).chars().map(|c| c as u32)),
    ];
    if let Some(instances) = &drawable.instances {
        words.push(fnv(instances.iter().flat_map(|instance| {
            instance
                .matrix
                .to_cols_array()
                .map(|v| (v as f32).to_bits())
        })));
    }
    fnv(words)
}

/// (meshes, vertices, triangles, hash) like the TypeScript `summary`.
type Summary = (usize, usize, usize, u32);

/// A model's [`Summary`], plus the per-mesh hashes for debugging.
fn summary_with(root: &Node, precision: Precision) -> (Summary, Vec<u32>) {
    fn visit(node: &Node, parent: DMat4, precision: Precision, out: &mut Vec<(usize, usize, u32)>) {
        let world = parent * compose(node.position, node.rotation, node.scale);
        if let Some(drawable) = &node.drawable {
            out.push((
                drawable.mesh.vertex_count(),
                drawable.mesh.triangle_count(),
                mesh_hash(node, world, precision),
            ));
        }
        for child in &node.children {
            visit(child, world, precision, out);
        }
    }
    let mut meshes = Vec::new();
    visit(root, DMat4::IDENTITY, precision, &mut meshes);
    let hashes: Vec<u32> = meshes.iter().map(|m| m.2).collect();
    (
        (
            meshes.len(),
            meshes.iter().map(|m| m.0).sum(),
            meshes.iter().map(|m| m.1).sum(),
            fnv(hashes.iter().copied()),
        ),
        hashes,
    )
}

fn summary(root: &Node) -> (Summary, Vec<u32>) {
    summary_with(root, Precision::Grid)
}

pub(super) struct CoverCase {
    kind: CoverKind,
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    color: u32,
    debris_seed: Option<f64>,
    hits: &'static [TimberHit],
    join: Option<TimberJoin>,
    background: bool,
    stage: u32,
    expected: Summary,
    exact: u32,
    timber_parts: (usize, u32),
    tree: Option<(u32, u32)>,
}

fn timber_hash(parts: &[TimberPart]) -> u32 {
    fnv(parts.iter().flat_map(|p| {
        let mut words = vec![
            u32::from(p.kind == TimberPartKind::Post),
            p.index as u32,
            p.damage,
            p.marks.len() as u32,
            p.damage_seed as u32,
        ];
        words.extend(p.marks.iter().map(|m| m.seed as u32));
        words.extend([p.x, p.y, p.z, p.w, p.h, p.d, p.lean].map(|v| (v as f32).to_bits()));
        words
    }))
}

#[test]
fn random_matches_typescript() {
    let mut a = Random::new(812.0);
    let mut b = Random::new(4_294_967_295.0);
    let mut c = Random::new(-1_234_567.0);
    assert_eq!([a.next(), a.next(), a.next()], RANDOM_812);
    assert_eq!([b.next(), b.next()], RANDOM_MAX);
    assert_eq!([c.next(), c.next()], RANDOM_NEGATIVE);
}

#[test]
fn cover_models_match_typescript() {
    let mut failures = Vec::new();
    let mut exact = 0;
    for (index, case) in COVER_CASES.iter().enumerate() {
        let shape = CoverShape {
            kind: case.kind,
            x: case.x,
            z: case.z,
            w: case.w,
            d: case.d,
            h: case.h,
            color: case.color,
            debris_seed: case.debris_seed,
            timber_hits: case.hits.to_vec(),
            timber_join: case.join,
        };
        let detail = if case.background {
            TreeDetail::Background
        } else {
            TreeDetail::Full
        };
        let model = cover_model(&shape, detail, case.stage);
        let (actual, hashes) = summary(&model);
        // The members and tree traits the TypeScript kept in `userData`.
        let parts = if case.kind == CoverKind::Timber {
            let wall = TimberWall {
                x: case.x,
                z: case.z,
                w: case.w,
                h: case.h,
                d: case.d,
                color: case.color,
                hits: case.hits,
                join: case.join,
            };
            timber_parts(&wall, case.stage)
        } else {
            Vec::new()
        };
        let parts = (parts.len(), timber_hash(&parts));
        let tree = (case.kind == CoverKind::Tree).then(|| {
            let proportions = tree_proportions(case.x, case.z, case.w, case.d, case.h);
            (proportions.family, proportions.seed)
        });
        if summary_with(&model, Precision::Exact).0.3 == case.exact {
            exact += 1;
        }
        if actual != case.expected || parts != case.timber_parts || tree != case.tree {
            failures.push(format!(
                "#{index} {:?} ({}, {}) stage {} background {}: {actual:?} vs {:?}, parts {parts:?} vs {:?}, tree {:?} vs {:?}, meshes {hashes:08x?}",
                case.kind,
                case.x,
                case.z,
                case.stage,
                case.background,
                case.expected,
                case.timber_parts,
                tree,
                case.tree
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} covers differ:\n{}",
        failures.len(),
        COVER_CASES.len(),
        failures.join("\n")
    );
    println!("{exact} of {} covers match bit for bit", COVER_CASES.len());
}

#[test]
fn every_cover_kind_is_covered() {
    for kind in CoverKind::ALL {
        assert!(COVER_CASES.iter().any(|case| case.kind == kind), "{kind:?}");
    }
    let families: std::collections::BTreeSet<u32> = COVER_CASES
        .iter()
        .filter_map(|case| case.tree.map(|t| t.0))
        .collect();
    assert_eq!(families.len(), TREE_FAMILIES.len());
}

#[test]
fn pickups_match_typescript() {
    for &(kind, expected) in PICKUP_CASES {
        assert_eq!(summary(&pickup_cube(kind)).0, expected, "{kind:?}");
    }
}

#[test]
fn flags_match_typescript() {
    let (actual, hashes) = summary(&flags_model());
    assert_eq!(actual, FLAGS, "{hashes:08x?}");
}

#[test]
fn debris_geometry_matches_typescript() {
    let hash = |mesh: &Mesh| {
        fnv([
            values_hash(&mesh.positions, Precision::Grid),
            values_hash(&mesh.normals, Precision::Grid),
            values_hash(&mesh.uvs, Precision::Grid),
            mesh.indices.as_ref().map_or(0, |i| fnv(i.iter().copied())),
        ])
    };
    assert_eq!(
        [
            hash(&barrel_scrap_geometry(BarrelScrap::Shell)),
            hash(&barrel_scrap_geometry(BarrelScrap::Lid))
        ],
        BARREL_SCRAPS
    );
    let trunk = trunk_fragment();
    let parts: Vec<_> = trunk
        .children
        .iter()
        .map(|c| c.drawable.as_ref().unwrap())
        .collect();
    // The three submeshes share the vertex arrays and split the index like groups.
    let mut whole = (*parts[0].mesh).clone();
    whole.indices = Some(
        parts
            .iter()
            .flat_map(|p| p.mesh.indices.clone().unwrap())
            .collect(),
    );
    let mut groups = Vec::new();
    let mut start = 0u32;
    for (i, part) in parts.iter().enumerate() {
        let count = part.mesh.indices.as_ref().unwrap().len() as u32;
        groups.extend([start, count, i as u32]);
        start += count;
    }
    let materials: Vec<String> = parts.iter().map(|p| descriptor(&p.material)).collect();
    assert_eq!(
        (hash(&whole), fnv(groups), materials.join(";").as_str()),
        TRUNK_FRAGMENT
    );
}

#[test]
fn tree_damage_sheds_boughs_by_stage() {
    let shape = TreeShape {
        x: -36.0,
        z: -45.0,
        w: 2.6,
        d: 2.6,
        h: 5.8,
    };
    let tree = tree_model(&shape, TreeDetail::Full);
    let crown = tree.find(tree_part::CROWN).unwrap();
    let stages: Vec<_> = crown
        .children
        .iter()
        .filter_map(branch_drop_stage)
        .collect();
    assert!(stages.contains(&1) && stages.contains(&2));
    // Presentation's tree joints show the cut surface only once the tree is felled.
    assert!(!tree.find(tree_part::CUT_SURFACE).unwrap().visible);
}

/// Tower rubble keeps the tower's concrete footing and timber, never its clapboard
/// cabin. A browser can't tell this from texture downloads: village houses load the
/// same files.
#[test]
fn tower_rubble_wears_concrete_and_timber() {
    let shape = |kind| CoverShape {
        kind,
        x: -10.2,
        z: 28.0,
        w: 1.3,
        d: 3.0,
        h: 1.25,
        color: 0xbd8a4a,
        debris_seed: Some(7.0),
        timber_hits: Vec::new(),
        timber_join: None,
    };
    let textures = |kind| node_textures(&cover_model(&shape(kind), TreeDetail::Full, 0));
    let tower = textures(CoverKind::Tower);
    assert!(tower.contains(&TextureSource::File(building_kit::CLAPBOARD_TEXTURE)));
    assert_eq!(
        textures(CoverKind::Rubble),
        [
            TextureSource::File(CONCRETE_TEXTURE),
            TextureSource::File(timber_model::TIMBER_TEXTURE),
        ]
    );
}

#[test]
fn wreck_aging_darkens_over_two_and_a_half_seconds() {
    assert_eq!(wreck_brightness(0.0), 0.8);
    assert!((wreck_brightness(1.25) - 0.5).abs() < 1e-12);
    assert!((wreck_brightness(10.0) - 0.2).abs() < 1e-12);
}

// ---- Expected values printed by the TypeScript reference script. ----
pub(super) const RANDOM_812: [f64; 3] =
    [0.5522837908938527, 0.5438373878132552, 0.34959222935140133];

pub(super) const RANDOM_MAX: [f64; 2] = [0.8964226141106337, 0.189478256739676];

pub(super) const RANDOM_NEGATIVE: [f64; 2] = [0.1790107295382768, 0.05965530825778842];

#[rustfmt::skip]
pub(super) const COVER_CASES: &[CoverCase] = &[
    CoverCase { kind: CoverKind::Boundary, x: -60.5, z: 0.0, w: 1.0, d: 122.0, h: 2.2, color: 0xa68c68, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 324, 108, 0x065494e2), exact: 0x15c22f33, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Boundary, x: 0.0, z: -60.5, w: 122.0, d: 1.0, h: 2.2, color: 0xa68c68, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 324, 108, 0xfdd1dfd6), exact: 0x365dcc23, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: -45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2916, 972, 0xcd2ecb37), exact: 0x9144893c, timber_parts: (0, 0x811c9dc5), tree: Some((3, 2721558820)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: -45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x562fda98), exact: 0xa8603d22, timber_parts: (0, 0x811c9dc5), tree: Some((3, 2721558820)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: -9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2916, 972, 0xf8444986), exact: 0x07c5fb27, timber_parts: (0, 0x811c9dc5), tree: Some((4, 3929240884)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: -9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x22755baa), exact: 0x75afa7b3, timber_parts: (0, 0x811c9dc5), tree: Some((4, 3929240884)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: 9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0x372d3c87), exact: 0x08462286, timber_parts: (0, 0x811c9dc5), tree: Some((1, 365726412)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: 9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x58776cb4), exact: 0xf752649d, timber_parts: (0, 0x811c9dc5), tree: Some((1, 365726412)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: 45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0x4c9beb26), exact: 0xe0c27f30, timber_parts: (0, 0x811c9dc5), tree: Some((2, 1573408476)) },
    CoverCase { kind: CoverKind::Tree, x: -36.0, z: 45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x0207216f), exact: 0x06782495, timber_parts: (0, 0x811c9dc5), tree: Some((2, 1573408476)) },
    CoverCase { kind: CoverKind::House, x: -45.0, z: -39.0, w: 5.0, d: 6.0, h: 4.6, color: 0xb87b4c, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (101, 20166, 6722, 0x6a401755), exact: 0xc9de5a8e, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::House, x: -45.0, z: -13.0, w: 5.0, d: 6.0, h: 4.6, color: 0xb87b4c, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (101, 20658, 6886, 0x9cc531b4), exact: 0x93bad454, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::House, x: -45.0, z: 13.0, w: 5.0, d: 6.0, h: 4.6, color: 0xb87b4c, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (101, 20658, 6886, 0x03c7e43e), exact: 0x132fd660, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::House, x: -45.0, z: 39.0, w: 5.0, d: 6.0, h: 4.6, color: 0xb87b4c, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (101, 20166, 6722, 0xc8599f15), exact: 0x6c5a7784, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::House, x: -17.0, z: -46.0, w: 7.0, d: 5.0, h: 5.2, color: 0xc78b50, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (101, 18810, 6270, 0x827c884e), exact: 0xeeb9d78f, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Tree, x: -23.0, z: -28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2916, 972, 0x7bbf1f9c), exact: 0x6f08a204, timber_parts: (0, 0x811c9dc5), tree: Some((3, 296335492)) },
    CoverCase { kind: CoverKind::Tree, x: -23.0, z: -28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x13aad833), exact: 0xe40321ad, timber_parts: (0, 0x811c9dc5), tree: Some((3, 296335492)) },
    CoverCase { kind: CoverKind::Timber, x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 0, expected: (6, 252, 84, 0x7f9f8e88), exact: 0x179942cd, timber_parts: (5, 0x99ffff78), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 1, expected: (7, 477, 159, 0xd0afb454), exact: 0x6757391a, timber_parts: (5, 0x8ed47585), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 2, expected: (8, 711, 237, 0xce1ddb60), exact: 0x8caca910, timber_parts: (5, 0x8a018b0f), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 3, expected: (9, 945, 315, 0x4fbbcac6), exact: 0xeca02fcf, timber_parts: (5, 0xb3ab6651), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 0, expected: (8, 360, 120, 0xff252d42), exact: 0x6dd7be0b, timber_parts: (6, 0xe655edc3), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 1, expected: (9, 603, 201, 0x3f941ae0), exact: 0xa7b275ce, timber_parts: (6, 0xd33db945), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 2, expected: (10, 801, 267, 0xb5ca236c), exact: 0x42b1cc46, timber_parts: (6, 0xde038080), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 3, expected: (11, 1059, 353, 0x4f0687bf), exact: 0x6587c925, timber_parts: (6, 0x0f889ca1), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, color: 0x805336, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), background: false, stage: 0, expected: (2, 108, 36, 0xd4b69ecf), exact: 0x625990e6, timber_parts: (1, 0xb90f5fd3), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, color: 0x805336, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.28800000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.072, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), background: false, stage: 1, expected: (2, 342, 114, 0x423c03a6), exact: 0xfdffeb30, timber_parts: (1, 0x954fb765), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, color: 0x805336, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.28800000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.10800000000000004, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.072, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.55, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), background: false, stage: 2, expected: (2, 528, 176, 0x1a650d33), exact: 0x7d619f7b, timber_parts: (1, 0x7576cc39), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, color: 0x805336, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.28800000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.16800000000000004, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.048000000000000036, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.072, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.19199999999999998, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -0.55, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), background: false, stage: 3, expected: (2, 780, 260, 0x4096462a), exact: 0x20fe5b68, timber_parts: (1, 0x89bced13), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 0, expected: (6, 252, 84, 0xe2fe0040), exact: 0x8e1c6f87, timber_parts: (5, 0xfb32fb69), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.24053333333333327, size: 1.5 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 1, expected: (7, 501, 167, 0x8f41534f), exact: 0xb410747f, timber_parts: (5, 0x27e647f2), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.3608, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.24053333333333327, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 2, expected: (9, 693, 231, 0x3a8020ed), exact: 0x3b6c3d25, timber_parts: (5, 0x7586be2b), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5612444444444445, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.16035555555555564, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.24053333333333327, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.641422222222222, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), background: false, stage: 3, expected: (10, 936, 312, 0x75ebfded), exact: 0x1245614d, timber_parts: (5, 0x7c236fe8), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 0, expected: (8, 360, 120, 0xdb065484), exact: 0x571d159e, timber_parts: (6, 0xd0b6172f), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.24053333333333327, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 1, expected: (9, 624, 208, 0x6467cd9d), exact: 0x2668220e, timber_parts: (6, 0xf4c474a7), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.3608, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.24053333333333327, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 2, expected: (10, 837, 279, 0x10f19a49), exact: 0xbf983c05, timber_parts: (6, 0x6d7ad42a), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5612444444444445, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.16035555555555564, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.24053333333333327, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.641422222222222, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 3, expected: (11, 1098, 366, 0x24a3c999), exact: 0xe7bd8edd, timber_parts: (6, 0xd3bc87a1), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 0, expected: (6, 252, 84, 0x4b2f8684), exact: 0x438cffda, timber_parts: (5, 0x1fce39f2), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.24053333333333327, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 1, expected: (7, 495, 165, 0xcda392b5), exact: 0xbf1aa157, timber_parts: (5, 0x9de6bebd), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.3608, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.24053333333333327, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 2, expected: (8, 705, 235, 0xd9715163), exact: 0x47ab729c, timber_parts: (5, 0x84d19227), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5612444444444445, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.16035555555555564, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.24053333333333327, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.641422222222222, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 3, expected: (9, 957, 319, 0xe05ca523), exact: 0x5e5dd7dd, timber_parts: (5, 0x94bdb9cb), tree: None },
    CoverCase { kind: CoverKind::Drum, x: -25.0, z: -27.0, w: 1.2, d: 1.2, h: 1.7, color: 0xff5b24, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (4, 304, 192, 0x24db1f17), exact: 0xd0db9afa, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Tree, x: -23.0, z: 28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0x254cd9ca), exact: 0xe0c8d503, timber_parts: (0, 0x811c9dc5), tree: Some((3, 3998631780)) },
    CoverCase { kind: CoverKind::Tree, x: -23.0, z: 28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x7c690d54), exact: 0xbf9b1e59, timber_parts: (0, 0x811c9dc5), tree: Some((3, 3998631780)) },
    CoverCase { kind: CoverKind::Timber, x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (8, 360, 120, 0x44732394), exact: 0x9b9e6838, timber_parts: (6, 0xe702eef3), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.1840000000000002, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.296, y: 0.72, z: 0.45, size: 1.5 }], join: None, background: false, stage: 1, expected: (9, 621, 207, 0x06c0233b), exact: 0xc1cfa542, timber_parts: (6, 0x219afe4f), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.1840000000000002, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.4440000000000002, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.296, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 1.9500000000000002, y: 1.46, z: 0.45, size: 1.8 }], join: None, background: false, stage: 2, expected: (10, 825, 275, 0x892b11e8), exact: 0x70f5ce57, timber_parts: (6, 0x861456f0), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.1840000000000002, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.6906666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.19733333333333347, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.296, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.7893333333333332, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 1.9500000000000002, y: 2.2, z: 0.45, size: 1.8 }], join: None, background: false, stage: 3, expected: (11, 1059, 353, 0x14271662), exact: 0x375848c9, timber_parts: (6, 0xf7371433), tree: None },
    CoverCase { kind: CoverKind::Tower, x: -12.75, z: 28.0, w: 6.0, d: 5.0, h: 7.5, color: 0xbd864a, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (190, 22452, 8104, 0x1e6c6cfe), exact: 0x659720e2, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: -45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0x1d46f169), exact: 0xa3413d98, timber_parts: (0, 0x811c9dc5), tree: Some((1, 1573408452)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: -45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0xe71d5763), exact: 0xdc4453d3, timber_parts: (0, 0x811c9dc5), tree: Some((1, 1573408452)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: -9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2952, 984, 0xe062255c), exact: 0x02f2c2a0, timber_parts: (0, 0x811c9dc5), tree: Some((3, 365726420)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: -9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0xb8f7b964), exact: 0xa4409536, timber_parts: (0, 0x811c9dc5), tree: Some((3, 365726420)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: 9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2952, 984, 0x8e065515), exact: 0xaeca132a, timber_parts: (0, 0x811c9dc5), tree: Some((4, 3929240876)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: 9.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x965fc365), exact: 0x78107fd9, timber_parts: (0, 0x811c9dc5), tree: Some((4, 3929240876)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: 45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2340, 780, 0x08ec8d20), exact: 0x00c77657, timber_parts: (0, 0x811c9dc5), tree: Some((2, 2721558844)) },
    CoverCase { kind: CoverKind::Tree, x: 36.0, z: 45.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x5f06662e), exact: 0x555064cd, timber_parts: (0, 0x811c9dc5), tree: Some((2, 2721558844)) },
    CoverCase { kind: CoverKind::Tree, x: 23.0, z: -28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0x7632f0a0), exact: 0xb46e87a1, timber_parts: (0, 0x811c9dc5), tree: Some((1, 3998631804)) },
    CoverCase { kind: CoverKind::Tree, x: 23.0, z: -28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x9d181be8), exact: 0x580bca99, timber_parts: (0, 0x811c9dc5), tree: Some((1, 3998631804)) },
    CoverCase { kind: CoverKind::Timber, x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 0, expected: (8, 360, 120, 0xd000716a), exact: 0xe5e1a01f, timber_parts: (6, 0x01984a9b), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 1, expected: (9, 588, 196, 0x53249ef6), exact: 0xc95e6a75, timber_parts: (6, 0x7349bc65), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 2, expected: (10, 816, 272, 0xc516abfb), exact: 0x46df5679, timber_parts: (6, 0x3b73f228), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), background: false, stage: 3, expected: (11, 1065, 355, 0x47ba8871), exact: 0x5e96a62e, timber_parts: (6, 0x3666d305), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 0, expected: (6, 252, 84, 0x36972654), exact: 0x979942cd, timber_parts: (5, 0xf9065ec5), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 1, expected: (7, 504, 168, 0xf4a8a299), exact: 0x51408ca7, timber_parts: (5, 0x138d25f0), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 2, expected: (9, 705, 235, 0xe08da226), exact: 0xade92e33, timber_parts: (5, 0x716da465), tree: None },
    CoverCase { kind: CoverKind::Timber, x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), background: false, stage: 3, expected: (10, 954, 318, 0x79e31c80), exact: 0xb1e66e3b, timber_parts: (5, 0x1e920486), tree: None },
    CoverCase { kind: CoverKind::Tree, x: 23.0, z: 28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2340, 780, 0x078fdf4d), exact: 0x6b153419, timber_parts: (0, 0x811c9dc5), tree: Some((1, 296335516)) },
    CoverCase { kind: CoverKind::Tree, x: 23.0, z: 28.0, w: 2.6, d: 2.6, h: 6.0, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x65a9537b), exact: 0x0eab0cb0, timber_parts: (0, 0x811c9dc5), tree: Some((1, 296335516)) },
    CoverCase { kind: CoverKind::Boundary, x: -60.5, z: 0.0, w: 1.0, d: 122.0, h: 1.2, color: 0x879698, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 324, 108, 0x148e54e2), exact: 0xdd1bb882, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Boundary, x: 0.0, z: -60.5, w: 122.0, d: 1.0, h: 1.2, color: 0x879698, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 324, 108, 0xb50a5fd6), exact: 0x206dd692, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Container, x: -30.0, z: -32.0, w: 6.0, d: 14.0, h: 3.6, color: 0xd37c38, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (110, 35340, 11784, 0xc95b00cc), exact: 0x199b3cb4, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Container, x: -13.0, z: -8.0, w: 12.0, d: 5.0, h: 3.6, color: 0x6689ad, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (99, 31776, 10596, 0xe43a1bb1), exact: 0xcc9ec0e9, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -28.4, z: -22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (11, 264, 132, 0xee012fe8), exact: 0x6e8679f5, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -28.4, z: -22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 1, expected: (23, 396, 240, 0x0b26bc80), exact: 0x6a4546ec, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -28.4, z: -22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 2, expected: (31, 588, 336, 0x9b17b7d6), exact: 0x19301545, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -31.6, z: -22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (11, 264, 132, 0xff639390), exact: 0x1c1706b0, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -31.6, z: -22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 1, expected: (23, 396, 240, 0x778f6585), exact: 0x9da9bd82, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -31.6, z: -22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 2, expected: (33, 636, 360, 0x3c5ec1a8), exact: 0x1b4664dd, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -28.4, z: 22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (11, 264, 132, 0xc7412fe8), exact: 0x13321063, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -28.4, z: 22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 1, expected: (23, 396, 240, 0x8f8588b5), exact: 0x374309e2, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -28.4, z: 22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 2, expected: (32, 612, 348, 0xd5a0e67d), exact: 0xd6efc608, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -31.6, z: 22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (11, 264, 132, 0xeb239390), exact: 0xc4fe68a6, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -31.6, z: 22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 1, expected: (23, 396, 240, 0x30da315c), exact: 0x2fe8aa4c, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -31.6, z: 22.0, w: 3.1, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 2, expected: (32, 612, 348, 0xdbefa384), exact: 0x732a2407, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Concrete, x: -10.0, z: -55.0, w: 8.0, d: 1.1, h: 1.5, color: 0xb5b5a5, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (8, 2592, 864, 0x6f19440c), exact: 0x5399a12d, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -4.0, z: -20.0, w: 3.0, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (11, 264, 132, 0xf84d3884), exact: 0xba9c3d0f, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -4.0, z: -20.0, w: 3.0, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 1, expected: (23, 396, 240, 0xdf6fb43a), exact: 0x17580f77, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Cargo, x: -4.0, z: -20.0, w: 3.0, d: 3.0, h: 2.6, color: 0xb88b53, debris_seed: None, hits: &[], join: None, background: false, stage: 2, expected: (32, 612, 348, 0x15bab6d5), exact: 0x0154dbaa, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Concrete, x: -43.0, z: 0.0, w: 1.2, d: 10.0, h: 1.7, color: 0xb5b5a5, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (10, 3240, 1080, 0xb999d1d4), exact: 0xa7c0f4b0, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: -25.0, z: -12.0, w: 14.0, d: 8.0, h: 4.6, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x3fb8cc30), exact: 0x2373ee6a, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: -25.0, z: 12.0, w: 14.0, d: 8.0, h: 3.8, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0xf4c84c5c), exact: 0xce2a4d6d, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: -4.0, z: -28.0, w: 16.0, d: 9.0, h: 4.8, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x2d20965d), exact: 0x41dddc1d, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: -40.5, z: -37.0, w: 10.0, d: 12.0, h: 4.2, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x4ee78267), exact: 0x45241762, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: -24.5, z: -37.0, w: 10.0, d: 12.0, h: 3.6, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0xe61b4146), exact: 0xcd34058c, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: 10.0, z: -47.0, w: 13.0, d: 7.0, h: 3.4, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x9e137136), exact: 0x375e06a0, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -41.7, z: 15.3, w: 1.71, d: 1.71, h: 1.71, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 48, 36, 0xc3a20100), exact: 0x5cc7c06c, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -42.25, z: 12.45, w: 1.8, d: 1.8, h: 1.845, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (4, 219, 228, 0xe86b1cc0), exact: 0x8b347dd7, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -41.85, z: 9.35, w: 1.71, d: 1.71, h: 1.62, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 48, 36, 0x00beb937), exact: 0x8e694bcc, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -42.5, z: 6.7, w: 1.71, d: 1.71, h: 1.71, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 48, 36, 0xf720e731), exact: 0x3d987e93, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -45.25, z: 13.85, w: 1.8, d: 1.8, h: 1.845, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 48, 36, 0x8f7cf841), exact: 0xa14692a3, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -44.7, z: 10.8, w: 1.71, d: 1.71, h: 1.62, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (4, 219, 228, 0x973c6666), exact: 0x4c19a674, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -45.4, z: 7.9, w: 1.71, d: 1.71, h: 1.71, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 48, 36, 0xd24250fc), exact: 0xd5685ee1, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: -45.05, z: 4.75, w: 1.8, d: 1.8, h: 1.845, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (4, 219, 228, 0x40a5ab4c), exact: 0x69b052bd, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Hedgehog, x: -15.8, z: -22.0, w: 2.32, d: 2.56, h: 2.16, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (19, 584, 324, 0x7a8b4f0b), exact: 0x75831ec2, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: 25.0, z: 12.0, w: 14.0, d: 8.0, h: 4.6, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x7f18cc30), exact: 0x2373ee6a, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: 25.0, z: -12.0, w: 14.0, d: 8.0, h: 3.8, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x48a84c5c), exact: 0xce2a4d6d, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: 4.0, z: 28.0, w: 16.0, d: 9.0, h: 4.8, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x1820965d), exact: 0x41dddc1d, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: 40.5, z: 37.0, w: 10.0, d: 12.0, h: 4.2, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x98067957), exact: 0x54182f24, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: 24.5, z: 37.0, w: 10.0, d: 12.0, h: 3.6, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0xf231ea48), exact: 0x32cc2ef7, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rock, x: -10.0, z: 47.0, w: 13.0, d: 7.0, h: 3.4, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 1392, 512, 0x09737136), exact: 0x375e06a0, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: 41.7, z: -15.3, w: 1.71, d: 1.71, h: 1.71, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (1, 48, 36, 0x8ba37188), exact: 0x5cc7c06c, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Teeth, x: 42.25, z: -12.45, w: 1.8, d: 1.8, h: 1.845, color: 0xd2bd99, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (4, 219, 228, 0x958c7320), exact: 0xb57f8c21, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Concrete, x: -42.0, z: -26.0, w: 3.2, d: 1.1, h: 2.2, color: 0xb9b3a5, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 972, 324, 0x50054e44), exact: 0x9a18fc29, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, color: 0xa66f46, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (8, 360, 120, 0x8dbbab5c), exact: 0x35944538, timber_parts: (6, 0x091e14d3), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, color: 0xa66f46, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.1840000000000002, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.296, size: 1.5 }], join: None, background: false, stage: 1, expected: (9, 603, 201, 0x1c57da7d), exact: 0x3d20c9d1, timber_parts: (6, 0xda630a27), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, color: 0xa66f46, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.1840000000000002, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.4440000000000002, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.296, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.9500000000000002, size: 1.8 }], join: None, background: false, stage: 2, expected: (10, 819, 273, 0xfb07f13a), exact: 0xd68c9977, timber_parts: (6, 0x2dcd92e4), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, color: 0xa66f46, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.1840000000000002, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.6906666666666669, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.19733333333333347, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.296, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.7893333333333332, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.9500000000000002, size: 1.8 }], join: None, background: false, stage: 3, expected: (11, 1101, 367, 0x03921ddd), exact: 0x9bd60c9c, timber_parts: (6, 0x2736c22f), tree: None },
    CoverCase { kind: CoverKind::Hedgehog, x: -42.0, z: 18.0, w: 2.9, d: 3.2, h: 2.7, color: 0x5d6870, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (19, 584, 324, 0xe12d89b8), exact: 0x41582ad0, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Tree, x: -42.0, z: 34.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0x01c69e7c), exact: 0xfb4ad2a2, timber_parts: (0, 0x811c9dc5), tree: Some((3, 2545210752)) },
    CoverCase { kind: CoverKind::Tree, x: -42.0, z: 34.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x55496ca8), exact: 0x2a3d5941, timber_parts: (0, 0x811c9dc5), tree: Some((3, 2545210752)) },
    CoverCase { kind: CoverKind::Tree, x: -34.0, z: 18.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0x8d6c0654), exact: 0x219e64e9, timber_parts: (0, 0x811c9dc5), tree: Some((5, 2472333600)) },
    CoverCase { kind: CoverKind::Tree, x: -34.0, z: 18.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0xf4b35e65), exact: 0x834741b5, timber_parts: (0, 0x811c9dc5), tree: Some((5, 2472333600)) },
    CoverCase { kind: CoverKind::Tree, x: -18.0, z: -42.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2376, 792, 0xa0f439c9), exact: 0x73ea18d7, timber_parts: (0, 0x811c9dc5), tree: Some((0, 404364656)) },
    CoverCase { kind: CoverKind::Tree, x: -18.0, z: -42.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0xd135731c), exact: 0xa1aaedcb, timber_parts: (0, 0x811c9dc5), tree: Some((0, 404364656)) },
    CoverCase { kind: CoverKind::Tree, x: -18.0, z: 42.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2916, 972, 0xd07b35c3), exact: 0x38e29341, timber_parts: (0, 0x811c9dc5), tree: Some((3, 3890602624)) },
    CoverCase { kind: CoverKind::Tree, x: -18.0, z: 42.0, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x5fa87861), exact: 0x7232e73a, timber_parts: (0, 0x811c9dc5), tree: Some((3, 3890602624)) },
    CoverCase { kind: CoverKind::Timber, x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (8, 360, 120, 0x26663808), exact: 0x55b0bca0, timber_parts: (6, 0xd3ce2333), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3440000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.33599999999999997, y: 0.72, z: 0.45, size: 1.5 }], join: None, background: false, stage: 1, expected: (9, 624, 208, 0x6da17b4a), exact: 0xc83a95ac, timber_parts: (6, 0x1f895817), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3440000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.5040000000000001, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.33599999999999997, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.2, y: 1.46, z: 0.45, size: 1.8 }], join: None, background: false, stage: 2, expected: (10, 840, 280, 0xdac37833), exact: 0x7673611c, timber_parts: (6, 0x8cef0658), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -1.3440000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7840000000000003, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.22400000000000017, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.33599999999999997, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8959999999999999, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.2, y: 2.2, z: 0.45, size: 1.8 }], join: None, background: false, stage: 3, expected: (11, 1080, 360, 0x5f6a248c), exact: 0xdf005b03, timber_parts: (6, 0x8173519b), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (8, 360, 120, 0x75800028), exact: 0x2a3cfaba, timber_parts: (6, 0xdc0b1de3), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.3440000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.33599999999999997, size: 1.5 }], join: None, background: false, stage: 1, expected: (9, 600, 200, 0x87de6539), exact: 0xcbe49106, timber_parts: (6, 0xa4313197), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.3440000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5040000000000001, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.33599999999999997, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -2.2, size: 1.8 }], join: None, background: false, stage: 2, expected: (10, 834, 278, 0x39ec98db), exact: 0x961e2b9a, timber_parts: (6, 0x14d00b64), tree: None },
    CoverCase { kind: CoverKind::Timber, x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, color: 0xb47a49, debris_seed: None, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.3440000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.7840000000000003, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.22400000000000017, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.33599999999999997, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.8959999999999999, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -2.2, size: 1.8 }], join: None, background: false, stage: 3, expected: (11, 1059, 353, 0x23da611e), exact: 0xbe3e0ff9, timber_parts: (6, 0x91f6518f), tree: None },
    CoverCase { kind: CoverKind::Concrete, x: -50.0, z: -36.0, w: 1.1, d: 3.2, h: 2.2, color: 0xb9b3a5, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (3, 972, 324, 0x72ff0684), exact: 0x23f495fb, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Hedgehog, x: -50.0, z: 12.0, w: 3.2, d: 2.9, h: 2.7, color: 0x5d6870, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (19, 584, 324, 0x757eb1df), exact: 0xbd60505f, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Tree, x: -30.0, z: 12.5, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0xa35d4fa3), exact: 0x6efc13f9, timber_parts: (0, 0x811c9dc5), tree: Some((1, 3358185078)) },
    CoverCase { kind: CoverKind::Tree, x: -30.0, z: 12.5, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x253b542d), exact: 0x08d1cd87, timber_parts: (0, 0x811c9dc5), tree: Some((1, 3358185078)) },
    CoverCase { kind: CoverKind::Tree, x: -28.63, z: 11.89, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2952, 984, 0x37c56d90), exact: 0x5868e018, timber_parts: (0, 0x811c9dc5), tree: Some((3, 2681602454)) },
    CoverCase { kind: CoverKind::Tree, x: -28.63, z: 11.89, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x2bc8f429), exact: 0x9ccafc5e, timber_parts: (0, 0x811c9dc5), tree: Some((3, 2681602454)) },
    CoverCase { kind: CoverKind::Tree, x: -27.259999999999998, z: 11.28, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0xc0e3cd8f), exact: 0xd8a8f63e, timber_parts: (0, 0x811c9dc5), tree: Some((0, 189093162)) },
    CoverCase { kind: CoverKind::Tree, x: -27.259999999999998, z: 11.28, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0xd9d7fc06), exact: 0x869e2d71, timber_parts: (0, 0x811c9dc5), tree: Some((0, 189093162)) },
    CoverCase { kind: CoverKind::Tree, x: -25.89, z: 10.67, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2340, 780, 0x0ec0e8c6), exact: 0xdaf2a896, timber_parts: (0, 0x811c9dc5), tree: Some((1, 3025205698)) },
    CoverCase { kind: CoverKind::Tree, x: -25.89, z: 10.67, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0xc381e944), exact: 0x1bf3cf3b, timber_parts: (0, 0x811c9dc5), tree: Some((1, 3025205698)) },
    CoverCase { kind: CoverKind::Tree, x: -24.52, z: 10.06, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0x6dc7b4c9), exact: 0xbb50f784, timber_parts: (0, 0x811c9dc5), tree: Some((4, 1574733294)) },
    CoverCase { kind: CoverKind::Tree, x: -24.52, z: 10.06, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0xe82c87c2), exact: 0x42360363, timber_parts: (0, 0x811c9dc5), tree: Some((4, 1574733294)) },
    CoverCase { kind: CoverKind::Tree, x: -23.15, z: 9.45, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2340, 780, 0xf9775c45), exact: 0xb0f2fb67, timber_parts: (0, 0x811c9dc5), tree: Some((2, 1897509358)) },
    CoverCase { kind: CoverKind::Tree, x: -23.15, z: 9.45, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0xd2544daf), exact: 0x9d516933, timber_parts: (0, 0x811c9dc5), tree: Some((2, 1897509358)) },
    CoverCase { kind: CoverKind::Tree, x: -21.78, z: 8.84, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0x47e5c13a), exact: 0x47d3abc5, timber_parts: (0, 0x811c9dc5), tree: Some((2, 2006853066)) },
    CoverCase { kind: CoverKind::Tree, x: -21.78, z: 8.84, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0xa89db78f), exact: 0x61768353, timber_parts: (0, 0x811c9dc5), tree: Some((2, 2006853066)) },
    CoverCase { kind: CoverKind::Tree, x: -20.41, z: 8.23, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2916, 972, 0x7fa8486f), exact: 0xbe90573b, timber_parts: (0, 0x811c9dc5), tree: Some((4, 1377663394)) },
    CoverCase { kind: CoverKind::Tree, x: -20.41, z: 8.23, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x2136a26c), exact: 0x9260eda5, timber_parts: (0, 0x811c9dc5), tree: Some((4, 1377663394)) },
    CoverCase { kind: CoverKind::Tree, x: -19.04, z: 7.62, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0xfe94ec32), exact: 0x1fc506d7, timber_parts: (0, 0x811c9dc5), tree: Some((5, 747689238)) },
    CoverCase { kind: CoverKind::Tree, x: -19.04, z: 7.62, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x0559fbe6), exact: 0x7db228b6, timber_parts: (0, 0x811c9dc5), tree: Some((5, 747689238)) },
    CoverCase { kind: CoverKind::Tree, x: -13.559999999999999, z: 5.18, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2304, 768, 0x2db9087c), exact: 0x2f9a7dc1, timber_parts: (0, 0x811c9dc5), tree: Some((0, 4224017118)) },
    CoverCase { kind: CoverKind::Tree, x: -13.559999999999999, z: 5.18, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 790, 272, 0x90b89fb7), exact: 0x7692d5bd, timber_parts: (0, 0x811c9dc5), tree: Some((0, 4224017118)) },
    CoverCase { kind: CoverKind::Tree, x: -9.45, z: 3.3499999999999996, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2952, 984, 0x0edc356e), exact: 0xf30ec4e3, timber_parts: (0, 0x811c9dc5), tree: Some((3, 1034839202)) },
    CoverCase { kind: CoverKind::Tree, x: -9.45, z: 3.3499999999999996, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x23c46142), exact: 0xd475c1d1, timber_parts: (0, 0x811c9dc5), tree: Some((3, 1034839202)) },
    CoverCase { kind: CoverKind::Tree, x: 9.730000000000004, z: -5.190000000000001, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0xb12f6e80), exact: 0x7a387f13, timber_parts: (0, 0x811c9dc5), tree: Some((5, 304429278)) },
    CoverCase { kind: CoverKind::Tree, x: 9.730000000000004, z: -5.190000000000001, w: 2.6, d: 2.6, h: 5.8, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x485ae1ce), exact: 0x7fc7d360, timber_parts: (0, 0x811c9dc5), tree: Some((5, 304429278)) },
    CoverCase { kind: CoverKind::Tree, x: 7.25, z: -3.5, w: 1.0, d: 1.1, h: 3.2, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (13, 2880, 960, 0xce7b9cdd), exact: 0xc6fa2447, timber_parts: (0, 0x811c9dc5), tree: Some((5, 466071039)) },
    CoverCase { kind: CoverKind::Tree, x: 7.25, z: -3.5, w: 1.0, d: 1.1, h: 3.2, color: 0x169f65, debris_seed: None, hits: &[], join: None, background: true, stage: 0, expected: (2, 648, 226, 0x1935e114), exact: 0x4fd3f7c5, timber_parts: (0, 0x811c9dc5), tree: Some((5, 466071039)) },
    CoverCase { kind: CoverKind::Rubble, x: -10.2, z: 28.0, w: 1.3, d: 3.0, h: 1.25, color: 0xbd8a4a, debris_seed: None, hits: &[], join: None, background: false, stage: 0, expected: (8, 1404, 492, 0xba01407f), exact: 0x2866f373, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rubble, x: -10.2, z: 28.0, w: 1.3, d: 3.0, h: 1.25, color: 0xbd8a4a, debris_seed: Some(7.0), hits: &[], join: None, background: false, stage: 0, expected: (8, 1404, 492, 0x7ff92750), exact: 0xfc68ff11, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rubble, x: -10.2, z: 28.0, w: 1.3, d: 3.0, h: 1.25, color: 0xbd8a4a, debris_seed: Some(123456789.0), hits: &[], join: None, background: false, stage: 0, expected: (8, 1404, 492, 0xa5a581de), exact: 0x5bae38eb, timber_parts: (0, 0x811c9dc5), tree: None },
    CoverCase { kind: CoverKind::Rubble, x: -10.2, z: 28.0, w: 1.3, d: 3.0, h: 1.25, color: 0xbd8a4a, debris_seed: Some(4000000000.0), hits: &[], join: None, background: false, stage: 0, expected: (10, 1452, 516, 0x56fe19f6), exact: 0x5f499f4b, timber_parts: (0, 0x811c9dc5), tree: None },
];

#[rustfmt::skip]
pub(super) const PICKUP_CASES: &[(PickupKind, Summary)] = &[
    (PickupKind::Spread, (2, 168, 84, 0x116f6655)),
    (PickupKind::Rocket, (2, 168, 84, 0x6f66851d)),
    (PickupKind::Ricochet, (2, 168, 84, 0x3dcba0b5)),
    (PickupKind::Piercing, (2, 168, 84, 0xe30061f5)),
    (PickupKind::Rapid, (1, 24, 12, 0xdc881a68)),
    (PickupKind::Shield, (1, 24, 12, 0xd1096b10)),
    (PickupKind::Speed, (1, 24, 12, 0x927352b0)),
    (PickupKind::Repair, (1, 24, 12, 0x101f9b28)),
    (PickupKind::Laser, (1, 24, 12, 0x2aac1510)),
];

pub(super) const FLAGS: Summary = (3, 290, 416, 0xf4a0e350);

pub(super) const BARREL_SCRAPS: [u32; 2] = [0x1f004bc3, 0x365b8543];

pub(super) const TRUNK_FRAGMENT: (u32, u32, &str) = (
    0x6547f0cb,
    0x885a65b0,
    "ffffff|textures/trees/bark.webp|0|0|0|1|0;ffffff|textures/trees/rings.webp|0|0|0|1|0;ffffff|textures/trees/rings.webp|0|0|0|1|0",
);
