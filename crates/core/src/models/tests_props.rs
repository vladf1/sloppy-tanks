//! Cover, tree and prop models: every cover kind builds at every damage stage
//! within a triangle budget, and the timber members and tree seeds the simulation
//! shares with them stay fixed gameplay data.

use super::*;
use crate::geometry::reference_tests::fnv;
use crate::scene::{Node, TextureSource};
use crate::sim::timber_layout::{
    TimberHit, TimberJoin, TimberPart, TimberPartKind, TimberWall, timber_parts,
};
use crate::sim::tree_proportions::tree_proportions;
use crate::sim::types::CoverKind;

/// Triangles drawn by a model, counting every instance of an instanced mesh.
fn drawn_triangles(node: &Node) -> usize {
    let own = node.drawable.as_ref().map_or(0, |drawable| {
        let copies = drawable.instances.as_ref().map_or(1, Vec::len);
        drawable.mesh.triangle_count() * copies
    });
    own + node.children.iter().map(drawn_triangles).sum::<usize>()
}

fn cover(kind: CoverKind, [w, d, h]: [f64; 3], color: u32) -> CoverShape {
    CoverShape {
        kind,
        x: -25.0,
        z: -12.0,
        w,
        d,
        h,
        color,
        debris_seed: Some(7.0),
        timber_hits: Vec::new(),
        timber_join: None,
    }
}

/// Every cover kind and look the maps place, at every damage stage it shows,
/// builds a non-empty model within a triangle budget (about 1.5 times what it
/// draws today). Covers stand in the main view, the shadow pass and, on the
/// harbor, the water reflection.
#[test]
fn every_cover_model_stays_within_its_triangle_budget() {
    let hits = [
        TimberHit {
            x: -1.3,
            y: 0.35,
            z: -0.45,
            size: 1.0,
        },
        TimberHit {
            x: 0.3,
            y: 1.1,
            z: 0.45,
            size: 1.5,
        },
    ];
    let hit = |mut shape: CoverShape| {
        shape.timber_hits = hits.to_vec();
        shape
    };
    let joined = |mut shape: CoverShape| {
        shape.timber_join = Some(TimberJoin {
            open_min: true,
            open_max: false,
            post: false,
        });
        shape
    };
    let full = TreeDetail::Full;
    // (label, shape, detail, last damage stage, triangle budget)
    #[rustfmt::skip]
    let cases = [
        ("village wall", cover(CoverKind::Boundary, [1.0, 122.0, 2.2], 0xa68c68), full, 0, 6_600),
        ("quarry wall", cover(CoverKind::Boundary, [1.0, 122.0, 2.2], 0xd2bd99), full, 0, 5_600),
        ("harbor wall", cover(CoverKind::Boundary, [1.0, 122.0, 1.2], 0x879698), full, 0, 50),
        ("concrete wall", cover(CoverKind::Boundary, [1.0, 122.0, 2.2], 0x8a8a8a), full, 0, 160),
        ("tree", cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65), full, 0, 1_500),
        ("background tree", cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65), TreeDetail::Background, 0, 340),
        ("cottage", hit(cover(CoverKind::House, [5.0, 6.0, 4.6], 0xb87b4c)), full, 2, 10_000),
        ("timber", hit(joined(cover(CoverKind::Timber, [4.1, 0.9, 2.8], 0xb47a49))), full, 3, 630),
        ("timber post", cover(CoverKind::Timber, [0.9, 0.9, 2.8], 0x805336), full, 3, 760),
        ("drum", cover(CoverKind::Drum, [1.2, 1.2, 1.7], 0xff5b24), full, 0, 1_400),
        ("tower", cover(CoverKind::Tower, [6.0, 5.0, 7.5], 0xbd864a), full, 0, 19_600),
        ("tower rubble", cover(CoverKind::Rubble, [1.3, 3.0, 1.25], 0xbd8a4a), full, 0, 1_000),
        ("cottage heap", cover(CoverKind::Rubble, [5.0, 6.0, 1.15], 0xb87b4c), full, 0, 3_700),
        ("container", cover(CoverKind::Container, [6.0, 14.0, 3.6], 0xd37c38), full, 0, 2_300),
        ("cargo crate", hit(cover(CoverKind::Cargo, [2.4, 2.4, 2.0], 0x8a6a3c)), full, 2, 5_800),
        ("concrete barrier", cover(CoverKind::Concrete, [8.0, 1.1, 1.5], 0xb5b5a5), full, 0, 300),
        ("rock", cover(CoverKind::Rock, [14.0, 8.0, 4.6], 0xd2bd99), full, 0, 3_500),
        ("boulder", cover(CoverKind::Rock, [2.2, 1.8, 1.4], 0xd2bd99), full, 0, 600),
        ("dragon's tooth", cover(CoverKind::Teeth, [1.71, 1.71, 1.71], 0xd2bd99), full, 0, 340),
        ("hedgehog", cover(CoverKind::Hedgehog, [2.32, 2.56, 2.16], 0xd2bd99), full, 0, 490),
    ];
    for kind in CoverKind::ALL {
        assert!(cases.iter().any(|case| case.1.kind == kind), "{kind:?}");
    }
    for (label, shape, detail, last_stage, budget) in &cases {
        for stage in 0..=*last_stage {
            let triangles = drawn_triangles(&cover_model(shape, *detail, stage));
            assert!(
                (1..=*budget).contains(&triangles),
                "{label} stage {stage}: {triangles} triangles, budget {budget}"
            );
        }
    }
}

/// A timber wall's members as presentation draws them and the break-up releases
/// them as physics fragments.
struct TimberCase {
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    hits: &'static [TimberHit],
    join: Option<TimberJoin>,
    stage: u32,
    /// Member count and [`timber_hash`].
    parts: (usize, u32),
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

/// Timber members (their places, damage and mark seeds) and tree families and
/// seeds are gameplay data: the members become fragment bodies and a tree's
/// family sizes its stump collider. These values come from the TypeScript game and
/// have not changed since; a deliberate change re-captures them.
#[test]
fn timber_members_and_tree_seeds_are_fixed() {
    for case in TIMBER_CASES {
        let wall = TimberWall {
            x: case.x,
            z: case.z,
            w: case.w,
            h: case.h,
            d: case.d,
            color: 0xb47a49,
            hits: case.hits,
            join: case.join,
        };
        let parts = timber_parts(&wall, case.stage);
        assert_eq!(
            (parts.len(), timber_hash(&parts)),
            case.parts,
            "timber ({}, {}) stage {}",
            case.x,
            case.z,
            case.stage
        );
    }
    let mut families = std::collections::BTreeSet::new();
    for &([x, z, w, d, h], expected) in TREE_SEEDS {
        let proportions = tree_proportions(x, z, w, d, h);
        assert_eq!(
            (proportions.family, proportions.seed),
            expected,
            "tree ({x}, {z})"
        );
        families.insert(proportions.family);
    }
    assert_eq!(families.len(), TREE_FAMILIES.len());
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
#[rustfmt::skip]
const TIMBER_CASES: &[TimberCase] = &[
    TimberCase { x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 0, parts: (5, 0x99ffff78) },
    TimberCase { x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 1, parts: (5, 0x8ed47585) },
    TimberCase { x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 2, parts: (5, 0x8a018b0f) },
    TimberCase { x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 3, parts: (5, 0xb3ab6651) },
    TimberCase { x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 0, parts: (6, 0xe655edc3) },
    TimberCase { x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 1, parts: (6, 0xd33db945) },
    TimberCase { x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 2, parts: (6, 0xde038080) },
    TimberCase { x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 3, parts: (6, 0x0f889ca1) },
    TimberCase { x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), stage: 0, parts: (1, 0xb90f5fd3) },
    TimberCase { x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.28800000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.072, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), stage: 1, parts: (1, 0x954fb765) },
    TimberCase { x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.28800000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.10800000000000004, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.072, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.55, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), stage: 2, parts: (1, 0x7576cc39) },
    TimberCase { x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.28800000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.16800000000000004, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.048000000000000036, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.072, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.19199999999999998, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -0.55, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: true }), stage: 3, parts: (1, 0x89bced13) },
    TimberCase { x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 0, parts: (5, 0xfb32fb69) },
    TimberCase { x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.24053333333333327, size: 1.5 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 1, parts: (5, 0x27e647f2) },
    TimberCase { x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.3608, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.24053333333333327, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 2, parts: (5, 0x7586be2b) },
    TimberCase { x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5612444444444445, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.16035555555555564, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.24053333333333327, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.641422222222222, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: true, open_max: false, post: false }), stage: 3, parts: (5, 0x7c236fe8) },
    TimberCase { x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 0, parts: (6, 0xd0b6172f) },
    TimberCase { x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.24053333333333327, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 1, parts: (6, 0xf4c474a7) },
    TimberCase { x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.3608, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.24053333333333327, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 2, parts: (6, 0x6d7ad42a) },
    TimberCase { x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5612444444444445, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.16035555555555564, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.24053333333333327, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.641422222222222, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 3, parts: (6, 0xd3bc87a1) },
    TimberCase { x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 0, parts: (5, 0x1fce39f2) },
    TimberCase { x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.24053333333333327, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 1, parts: (5, 0x9de6bebd) },
    TimberCase { x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.3608, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.24053333333333327, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 2, parts: (5, 0x84d19227) },
    TimberCase { x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 0.9621333333333333, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5612444444444445, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.16035555555555564, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.24053333333333327, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.641422222222222, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.6033333333333333, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 3, parts: (5, 0x94bdb9cb) },
    TimberCase { x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, hits: &[], join: None, stage: 0, parts: (6, 0xe702eef3) },
    TimberCase { x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.1840000000000002, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.296, y: 0.72, z: 0.45, size: 1.5 }], join: None, stage: 1, parts: (6, 0x219afe4f) },
    TimberCase { x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.1840000000000002, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.4440000000000002, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.296, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 1.9500000000000002, y: 1.46, z: 0.45, size: 1.8 }], join: None, stage: 2, parts: (6, 0x861456f0) },
    TimberCase { x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.1840000000000002, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.6906666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.19733333333333347, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.296, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.7893333333333332, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 1.9500000000000002, y: 2.2, z: 0.45, size: 1.8 }], join: None, stage: 3, parts: (6, 0xf7371433) },
    TimberCase { x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 0, parts: (6, 0x01984a9b) },
    TimberCase { x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 1, parts: (6, 0x7349bc65) },
    TimberCase { x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 2, parts: (6, 0x3b73f228) },
    TimberCase { x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: false, post: false }), stage: 3, parts: (6, 0x3666d305) },
    TimberCase { x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 0, parts: (5, 0xf9065ec5) },
    TimberCase { x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.3284, y: 0.72, z: 0.45, size: 1.5 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 1, parts: (5, 0x138d25f0) },
    TimberCase { x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.49260000000000015, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.3284, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.1525000000000003, y: 1.46, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 2, parts: (5, 0x716da465) },
    TimberCase { x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3136000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7662666666666669, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.21893333333333348, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.3284, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8757333333333333, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.1525000000000003, y: 2.2, z: 0.45, size: 1.8 }], join: Some(TimberJoin { open_min: false, open_max: true, post: false }), stage: 3, parts: (5, 0x1e920486) },
    TimberCase { x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, hits: &[], join: None, stage: 0, parts: (6, 0x091e14d3) },
    TimberCase { x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.1840000000000002, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.296, size: 1.5 }], join: None, stage: 1, parts: (6, 0xda630a27) },
    TimberCase { x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.1840000000000002, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.4440000000000002, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.296, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -1.9500000000000002, size: 1.8 }], join: None, stage: 2, parts: (6, 0x2dcd92e4) },
    TimberCase { x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.1840000000000002, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.6906666666666669, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.19733333333333347, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.296, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.7893333333333332, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -1.9500000000000002, size: 1.8 }], join: None, stage: 3, parts: (6, 0x2736c22f) },
    TimberCase { x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, hits: &[], join: None, stage: 0, parts: (6, 0xd3ce2333) },
    TimberCase { x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3440000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: 0.33599999999999997, y: 0.72, z: 0.45, size: 1.5 }], join: None, stage: 1, parts: (6, 0x1f895817) },
    TimberCase { x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3440000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.5040000000000001, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: 0.33599999999999997, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 2.2, y: 1.46, z: 0.45, size: 1.8 }], join: None, stage: 2, parts: (6, 0x8cef0658) },
    TimberCase { x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, hits: &[TimberHit { x: -1.3440000000000003, y: 0.35, z: -0.45, size: 1.0 }, TimberHit { x: -0.7840000000000003, y: 0.72, z: 0.45, size: 1.5 }, TimberHit { x: -0.22400000000000017, y: 1.0899999999999999, z: -0.45, size: 1.65 }, TimberHit { x: 0.33599999999999997, y: 1.46, z: 0.45, size: 1.8 }, TimberHit { x: 0.8959999999999999, y: 1.83, z: -0.45, size: 1.8 }, TimberHit { x: 2.2, y: 2.2, z: 0.45, size: 1.8 }], join: None, stage: 3, parts: (6, 0x8173519b) },
    TimberCase { x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, hits: &[], join: None, stage: 0, parts: (6, 0xdc0b1de3) },
    TimberCase { x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.3440000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: -0.33599999999999997, size: 1.5 }], join: None, stage: 1, parts: (6, 0xa4313197) },
    TimberCase { x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.3440000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.5040000000000001, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: -0.33599999999999997, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -2.2, size: 1.8 }], join: None, stage: 2, parts: (6, 0x14d00b64) },
    TimberCase { x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, hits: &[TimberHit { x: -0.45, y: 0.35, z: 1.3440000000000003, size: 1.0 }, TimberHit { x: 0.45, y: 0.72, z: 0.7840000000000003, size: 1.5 }, TimberHit { x: -0.45, y: 1.0899999999999999, z: 0.22400000000000017, size: 1.65 }, TimberHit { x: 0.45, y: 1.46, z: -0.33599999999999997, size: 1.8 }, TimberHit { x: -0.45, y: 1.83, z: -0.8959999999999999, size: 1.8 }, TimberHit { x: 0.45, y: 2.2, z: -2.2, size: 1.8 }], join: None, stage: 3, parts: (6, 0x91f6518f) },
];

/// Tree footprints `[x, z, w, d, h]` and their `(family, seed)`.
#[rustfmt::skip]
const TREE_SEEDS: &[([f64; 5], (u32, u32))] = &[
    ([-36.0, -45.0, 2.6, 2.6, 5.8], (3, 2721558820)),
    ([-36.0, -9.0, 2.6, 2.6, 5.8], (4, 3929240884)),
    ([-36.0, 9.0, 2.6, 2.6, 5.8], (1, 365726412)),
    ([-36.0, 45.0, 2.6, 2.6, 5.8], (2, 1573408476)),
    ([-23.0, -28.0, 2.6, 2.6, 6.0], (3, 296335492)),
    ([-23.0, 28.0, 2.6, 2.6, 6.0], (3, 3998631780)),
    ([36.0, -45.0, 2.6, 2.6, 5.8], (1, 1573408452)),
    ([36.0, -9.0, 2.6, 2.6, 5.8], (3, 365726420)),
    ([36.0, 9.0, 2.6, 2.6, 5.8], (4, 3929240876)),
    ([36.0, 45.0, 2.6, 2.6, 5.8], (2, 2721558844)),
    ([23.0, -28.0, 2.6, 2.6, 6.0], (1, 3998631804)),
    ([23.0, 28.0, 2.6, 2.6, 6.0], (1, 296335516)),
    ([-42.0, 34.0, 2.6, 2.6, 5.8], (3, 2545210752)),
    ([-34.0, 18.0, 2.6, 2.6, 5.8], (5, 2472333600)),
    ([-18.0, -42.0, 2.6, 2.6, 5.8], (0, 404364656)),
    ([-18.0, 42.0, 2.6, 2.6, 5.8], (3, 3890602624)),
    ([-30.0, 12.5, 2.6, 2.6, 5.8], (1, 3358185078)),
    ([-28.63, 11.89, 2.6, 2.6, 5.8], (3, 2681602454)),
    ([-27.259999999999998, 11.28, 2.6, 2.6, 5.8], (0, 189093162)),
    ([-25.89, 10.67, 2.6, 2.6, 5.8], (1, 3025205698)),
    ([-24.52, 10.06, 2.6, 2.6, 5.8], (4, 1574733294)),
    ([-23.15, 9.45, 2.6, 2.6, 5.8], (2, 1897509358)),
    ([-21.78, 8.84, 2.6, 2.6, 5.8], (2, 2006853066)),
    ([-20.41, 8.23, 2.6, 2.6, 5.8], (4, 1377663394)),
    ([-19.04, 7.62, 2.6, 2.6, 5.8], (5, 747689238)),
    ([-13.559999999999999, 5.18, 2.6, 2.6, 5.8], (0, 4224017118)),
    ([-9.45, 3.3499999999999996, 2.6, 2.6, 5.8], (3, 1034839202)),
    ([9.730000000000004, -5.190000000000001, 2.6, 2.6, 5.8], (5, 304429278)),
    ([7.25, -3.5, 1.0, 1.1, 3.2], (5, 466071039)),
];
