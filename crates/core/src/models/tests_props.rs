//! Cover, tree and prop models: every cover kind builds at every damage stage
//! within a triangle budget, and the timber members and tree seeds the simulation
//! shares with them stay fixed gameplay data.

use super::*;
use crate::geometry::reference_tests::fnv;
use crate::scene::{Node, TextureSource};
use crate::sim::structure::PartSet;
use crate::sim::timber_layout::{TimberJoin, TimberPart, TimberWall, timber_parts};
use crate::sim::tower_structure::{piece_look, tower_structure};
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
        timber_join: None,
        house_plan: None,
        broken: PartSet::EMPTY,
        cracked: PartSet::EMPTY,
    }
}

/// Every cover kind and look the maps place, at every damage stage it shows,
/// builds a non-empty model within a triangle budget (about 1.5 times what it
/// draws today). Covers stand in the main view, the shadow pass and, on the
/// harbor, the water reflection.
#[test]
fn every_cover_model_stays_within_its_triangle_budget() {
    // A timber wall with a beam shot out, a crate with a wall blown out.
    let hit = |mut shape: CoverShape| {
        shape.broken = PartSet::from_indices([1]);
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
    // A cottage with a run of its parts shot out, torn edges round the gap.
    let gapped = |mut shape: CoverShape| {
        shape.broken = PartSet::from_indices(5..14);
        shape
    };
    // A watchtower toppled off its footings: they and their shoes are left.
    let toppled = |mut shape: CoverShape| {
        shape.broken = PartSet::from_indices(2..tower_structure().parts.len());
        shape
    };
    let full = TreeDetail::Full;
    // (label, shape, detail, unused, triangle budget)
    #[rustfmt::skip]
    let cases = [
        ("village wall", cover(CoverKind::Boundary, [1.0, 122.0, 2.2], 0xa68c68), full, 0, 6_600),
        ("quarry wall", cover(CoverKind::Boundary, [1.0, 122.0, 2.2], 0xd2bd99), full, 0, 5_600),
        ("harbor wall", cover(CoverKind::Boundary, [1.0, 122.0, 1.2], 0x879698), full, 0, 50),
        ("concrete wall", cover(CoverKind::Boundary, [1.0, 122.0, 2.2], 0x8a8a8a), full, 0, 160),
        ("tree", cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65), full, 0, 1_500),
        // An oak's position: full-detail oaks grow their own branching (`grown_crown`).
        ("oak", CoverShape { x: -36.0, z: -45.0, ..cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65) }, full, 0, 1_500),
        ("background tree", cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65), TreeDetail::Background, 0, 340),
        ("cottage", cover(CoverKind::House, [5.0, 6.0, 4.6], 0xb87b4c), full, 0, 10_000),
        ("gapped cottage", gapped(cover(CoverKind::House, [5.0, 6.0, 4.6], 0xb87b4c)), full, 0, 14_000),
        ("timber", hit(joined(cover(CoverKind::Timber, [4.1, 0.9, 2.8], 0xb47a49))), full, 0, 630),
        ("timber post", cover(CoverKind::Timber, [0.9, 0.9, 2.8], 0x805336), full, 0, 760),
        ("drum", cover(CoverKind::Drum, [1.2, 1.2, 1.7], 0xff5b24), full, 0, 1_400),
        ("tower", cover(CoverKind::Tower, [6.0, 5.0, 7.5], 0xbd864a), full, 0, 19_600),
        ("toppled tower", toppled(cover(CoverKind::Tower, [6.0, 5.0, 7.5], 0xbd864a)), full, 0, 3_200),
        ("container", cover(CoverKind::Container, [6.0, 14.0, 3.6], 0xd37c38), full, 0, 2_300),
        ("cargo crate", hit(cover(CoverKind::Cargo, [2.4, 2.4, 2.0], 0x8a6a3c)), full, 0, 5_800),
        ("concrete barrier", cover(CoverKind::Concrete, [8.0, 1.1, 1.5], 0xb5b5a5), full, 0, 300),
        ("rock", cover(CoverKind::Rock, [14.0, 8.0, 4.6], 0xd2bd99), full, 0, 3_500),
        ("boulder", cover(CoverKind::Rock, [2.2, 1.8, 1.4], 0xd2bd99), full, 0, 600),
        ("dragon's tooth", cover(CoverKind::Teeth, [1.71, 1.71, 1.71], 0xd2bd99), full, 0, 340),
        ("hedgehog", cover(CoverKind::Hedgehog, [2.32, 2.56, 2.16], 0xd2bd99), full, 0, 490),
    ];
    for kind in CoverKind::ALL {
        assert!(cases.iter().any(|case| case.1.kind == kind), "{kind:?}");
    }
    for (label, shape, detail, _, budget) in &cases {
        let triangles = drawn_triangles(&cover_model(shape, *detail));
        assert!(
            (1..=*budget).contains(&triangles),
            "{label}: {triangles} triangles, budget {budget}"
        );
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
    join: Option<TimberJoin>,
    /// Member count and [`timber_hash`].
    parts: (usize, u32),
}

fn timber_hash(parts: &[TimberPart]) -> u32 {
    fnv(parts.iter().flat_map(|p| {
        let mut words = vec![p.kind as u32, p.index as u32, p.seed as u32];
        words.extend([p.x, p.y, p.z, p.w, p.h, p.d].map(|v| (v as f32).to_bits()));
        words
    }))
}

/// Timber members (their places and seeds) and tree families and
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
            join: case.join,
        };
        let parts = timber_parts(&wall);
        assert_eq!(
            (parts.len(), timber_hash(&parts)),
            case.parts,
            "timber ({}, {})",
            case.x,
            case.z,
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

/// A cover tree a shell snapped a limb off stands without it: the limb lies as a fallen
/// body of its own (`structure_debris`), never drawn twice.
#[test]
fn a_standing_tree_leaves_out_its_snapped_limbs() {
    use crate::sim::grown_layout::TreePart;
    use crate::sim::structure::Role;
    use crate::sim::tree_structure::tree_structure;
    for family in 0..6 {
        let (x, z) = (0..400)
            .map(|step| (-25.0 + f64::from(step) * 0.37, -12.0))
            .find(|&(x, z)| tree_proportions(x, z, 2.6, 2.6, 5.8).family == family)
            .unwrap();
        let shape = CoverShape {
            x,
            z,
            ..cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65)
        };
        let layout = tree_structure(x, z, 2.6, 2.6, 5.8);
        let limb = (0..layout.parts.len())
            .find(|&index| matches!(layout.parts[index].role, Role::Tree(TreePart::Limb(_))))
            .unwrap();
        let whole = drawn_triangles(&cover_model(&shape, TreeDetail::Full));
        assert!(
            whole <= 1_500,
            "{}: {whole} triangles, over a cover tree's budget",
            TREE_FAMILIES[family as usize]
        );
        let snapped = CoverShape {
            broken: PartSet::from_indices([limb]),
            ..shape
        };
        let standing = drawn_triangles(&cover_model(&snapped, TreeDetail::Full));
        let fallen = drawn_triangles(&structure_debris(&snapped, &PartSet::from_indices([limb])));
        assert!(
            standing < whole,
            "{}: {standing} of {whole} triangles stand",
            TREE_FAMILIES[family as usize]
        );
        assert!(
            fallen > 0,
            "{}: the limb is drawn fallen",
            TREE_FAMILIES[family as usize]
        );
    }
}

/// A felled tree's pieces leave out the boughs it already shed as it was hit: they fell
/// on their own and are not drawn again on the fallen crown.
#[test]
fn a_felled_tree_leaves_out_the_boughs_it_shed() {
    use crate::sim::grown_layout::TreePart;
    use crate::sim::structure::Role;
    use crate::sim::tree_structure::tree_structure;
    for family in 0..6 {
        let name = TREE_FAMILIES[family as usize];
        let (x, z) = (0..400)
            .map(|step| (-25.0 + f64::from(step) * 0.37, -12.0))
            .find(|&(x, z)| tree_proportions(x, z, 2.6, 2.6, 5.8).family == family)
            .unwrap();
        let shape = CoverShape {
            x,
            z,
            ..cover(CoverKind::Tree, [2.6, 2.6, 5.8], 0x169f65)
        };
        let layout = tree_structure(x, z, 2.6, 2.6, 5.8);
        // The trunk and every limb, falling together.
        let crown = PartSet::from_indices((0..layout.parts.len()).filter(|&index| {
            matches!(
                layout.parts[index].role,
                Role::Tree(TreePart::Trunk | TreePart::Limb(_))
            )
        }));
        let whole = drawn_triangles(&tree_debris(&shape, &crown, 0));
        let once = drawn_triangles(&tree_debris(&shape, &crown, 1));
        let twice = drawn_triangles(&tree_debris(&shape, &crown, 2));
        assert!(
            twice < once && once < whole,
            "{name}: {whole} whole, {once} after the first boughs fell, {twice} after the rest"
        );
    }
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

/// A toppled watchtower's footings keep its concrete and timber stubs, never its
/// clapboard cabin. A browser can't tell this from texture downloads: village houses
/// load the same files.
#[test]
fn a_toppled_towers_footings_wear_concrete_and_timber() {
    let shape = |broken| CoverShape {
        kind: CoverKind::Tower,
        x: -10.2,
        z: 28.0,
        w: 6.0,
        d: 5.0,
        h: 7.5,
        color: 0xbd8a4a,
        timber_join: None,
        house_plan: None,
        broken,
        cracked: PartSet::EMPTY,
    };
    let textures = |broken| node_textures(&cover_model(&shape(broken), TreeDetail::Full));
    let tower = textures(PartSet::EMPTY);
    assert!(tower.contains(&TextureSource::File(building_kit::CLAPBOARD_TEXTURE)));
    assert_eq!(
        textures(PartSet::from_indices(2..tower_structure().parts.len())),
        [
            TextureSource::File(CONCRETE_TEXTURE),
            TextureSource::File(timber_model::TIMBER_TEXTURE),
        ]
    );
}

/// The triangles of `node` in its own coordinates: corners and normals.
fn world_triangles(
    node: &Node,
    parent: glam::DMat4,
    out: &mut Vec<([glam::DVec3; 3], glam::DVec3)>,
) {
    let world = parent * node.local_matrix();
    if let Some(drawable) = &node.drawable {
        let mesh = &drawable.mesh;
        let point = |i: usize| {
            world.transform_point3(glam::DVec3::from_array(mesh.positions[i].map(f64::from)))
        };
        let triangles: Vec<[usize; 3]> = match &mesh.indices {
            Some(indices) => indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| i as usize))
                .collect(),
            None => (0..mesh.positions.len() / 3)
                .map(|t| [3 * t, 3 * t + 1, 3 * t + 2])
                .collect(),
        };
        for [a, b, c] in triangles {
            let corners = [point(a), point(b), point(c)];
            let face = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
            let normal =
                world.transform_vector3(glam::DVec3::from_array(mesh.normals[a].map(f64::from)));
            let face = if face.dot(normal) < 0.0 { -face } else { face };
            out.push((corners, face));
        }
    }
    for child in &node.children {
        world_triangles(child, world, out);
    }
}

/// A landed roof mostly comes to lie overturned, so what it shows from beneath carries
/// its damage too: boarding over most of its plan that follows the slopes, creases and
/// dents rather than one flat deck or an open shell.
#[test]
fn a_landed_roofs_underside_is_as_crumpled_as_its_sheet() {
    use crate::sim::tower_structure::{PART_COUNT, PieceLook};
    let structure = tower_structure();
    let roof = (PART_COUNT..PART_COUNT + 64)
        .find(|&index| piece_look(&structure, index) == Some(PieceLook::Roof))
        .unwrap();
    let mut triangles = Vec::new();
    world_triangles(
        &tower_debris(0xbd864a, &PartSet::from_indices([roof])),
        glam::DMat4::IDENTITY,
        &mut triangles,
    );
    let beneath: Vec<_> = triangles
        .iter()
        .filter(|(_, face)| face.length() > 1e-9 && face.normalize().y < -0.3)
        .collect();
    let area: f64 = beneath
        .iter()
        .map(|(_, face)| face.length() / 2.0 * -face.normalize().y)
        .sum();
    assert!(area > 15.0, "boarding under most of the roof ({area} m²)");
    // Away from the folds (the east end past the crease, the bent front eave), what
    // is seen from beneath (the lowest surface over each point) rises from the eaves
    // to the ridge as the sheet does, rather than lying as one deck at the eave line.
    let seen = |x: f64, z: f64| {
        beneath
            .iter()
            .filter_map(|(c, _)| {
                let flat = |p: glam::DVec3| glam::DVec2::new(p.x, p.z);
                let (a, b, q) = (flat(c[0]), flat(c[1]), flat(c[2]));
                let at = glam::DVec2::new(x, z);
                let area = (b - a).perp_dot(q - a);
                let u = (q - b).perp_dot(at - b) / area;
                let v = (a - q).perp_dot(at - q) / area;
                let w = 1.0 - u - v;
                (u >= 0.0 && v >= 0.0 && w >= 0.0).then(|| u * c[0].y + v * c[1].y + w * c[2].y)
            })
            .fold(f64::INFINITY, f64::min)
    };
    let heights: Vec<f64> = (0..12)
        .flat_map(|i| (0..9).map(move |k| (-2.4 + 0.25 * f64::from(i), -1.2 + 0.3 * f64::from(k))))
        .map(|(x, z)| seen(x, z))
        .filter(|y| y.is_finite())
        .collect();
    let (low, high) = heights
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &y| {
            (l.min(y), h.max(y))
        });
    assert!(
        heights.len() > 90,
        "boarding over the unfolded part ({} of 108 points)",
        heights.len()
    );
    assert!(
        high - low > 0.8,
        "the underside follows the slopes ({} m)",
        high - low
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
    TimberCase { x: -30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, join: Some(TimberJoin { open_min: true, open_max: false, post: false }), parts: (6, 0x14afe757) },
    TimberCase { x: -26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, join: Some(TimberJoin { open_min: false, open_max: false, post: false }), parts: (8, 0x29916c62) },
    TimberCase { x: -33.2, z: -33.0, w: 0.9, d: 0.9, h: 2.8, join: Some(TimberJoin { open_min: false, open_max: false, post: true }), parts: (2, 0x3ef762f3) },
    TimberCase { x: -33.2, z: -31.006666666666668, w: 0.9, d: 3.0066666666666664, h: 2.8, join: Some(TimberJoin { open_min: true, open_max: false, post: false }), parts: (6, 0x7e60c7e0) },
    TimberCase { x: -33.2, z: -28.0, w: 0.9, d: 3.0066666666666664, h: 2.8, join: Some(TimberJoin { open_min: false, open_max: false, post: false }), parts: (8, 0x00cc270e) },
    TimberCase { x: -33.2, z: -24.993333333333332, w: 0.9, d: 3.0066666666666664, h: 2.8, join: Some(TimberJoin { open_min: false, open_max: true, post: false }), parts: (6, 0x12c469ff) },
    TimberCase { x: -6.0, z: -13.0, w: 3.7, d: 0.9, h: 2.8, join: None, parts: (8, 0xcae49bbe) },
    TimberCase { x: 26.552500000000002, z: -33.0, w: 4.105, d: 0.9, h: 2.8, join: Some(TimberJoin { open_min: false, open_max: false, post: false }), parts: (8, 0x0e1a1162) },
    TimberCase { x: 30.6575, z: -33.0, w: 4.105, d: 0.9, h: 2.8, join: Some(TimberJoin { open_min: false, open_max: true, post: false }), parts: (6, 0x27e99e30) },
    TimberCase { x: -42.0, z: -18.0, w: 0.9, d: 3.7, h: 2.8, join: None, parts: (8, 0x144a4d46) },
    TimberCase { x: -12.0, z: -22.0, w: 4.2, d: 0.9, h: 2.8, join: None, parts: (8, 0xb308afe6) },
    TimberCase { x: -22.0, z: -12.0, w: 0.9, d: 4.2, h: 2.8, join: None, parts: (8, 0x0e052ec6) },
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
