//! Scenery compared with the TypeScript/Three.js implementation. The expected
//! dump below was printed by a Node script that built `VillageScenery`,
//! `HarborScenery`, `QuarryScenery`, `createArenaFloor`, `createSpawnPads`,
//! `shippingContainer` and `sandstoneFooting` with Three r185 and walked each
//! tree depth-first. Cargo crates have since been rebuilt from boards
//! ([`super::crate_model`]) and left the comparison. Each line is one node:
//!
//! `path|name|visible|children|position|quaternion|scale` (9 significant digits;
//! `=` for an identity transform), and for meshes `|V=vertices|I=indices|P=|N=|U=|
//! C=` FNV-1a hashes of the stored f32 position, normal, uv and RGB color bits,
//! `A=` the vertex alpha hash, `X=` custom attributes (`name:size:hash`), `M=` a
//! hash of the material summary (see `material_summary`), `F=cast,receive,
//! renderOrder,frustumCulled` and `INST=count,matrices,colors` for instanced
//! draws.
//!
//! The reflector target objects Three attaches under the water meshes are skipped.
//! The background forest's foliage is placed by Euler rotations, whose `sin`/`cos`
//! can differ from V8's in the last bit; its normals (unlike its positions) show
//! that in a few f32 ulps, so they are compared on a 1/65536 grid instead
//! ([`forest_normals_match_typescript_on_a_grid`]).
//!
//! The moored ships' cargo departs from the TypeScript on purpose: its fittings
//! are square boxes (`ship_container`), so the ships' painted batches are
//! compared without their geometry ([`square_ship_fittings`]).
//!
//! Set `SCENERY_DUMP=<file>` to write the Rust dump with full material summaries
//! for diffing against the script's output.

use std::fmt::Write as _;

use glam::DMat4;

use super::effects_scenery::{CHIMNEY_SMOKE, QUARRY_SOIL, SAND_DRIFT, WATER};
use super::harbor_models::{CargoShape, shipping_container};
use super::quarry_scenery::quarry_scenery;
use super::quarry_surfaces::{sand_drift_material, sandstone_footing, sandstone_material};
use super::*;
use crate::geometry::VERTEX_ALPHA;
use crate::geometry::reference_tests::fnv;
use crate::scene::{Effect, Material, Node, Shading, Side, TextureRef, TextureSource};
use crate::sim::quarry_rock_shape::quarry_rock_shape;

fn float_hash<'a>(values: impl IntoIterator<Item = &'a f32>) -> String {
    format!("{:08x}", fnv(values.into_iter().map(|v| v.to_bits())))
}

/// `Number(n.toPrecision(9)).toString()`, with tiny values as `0`.
fn num(n: f64) -> String {
    if n.abs() < 5e-10 {
        return "0".into();
    }
    let rounded: f64 = format!("{n:.8e}").parse().expect("number");
    if rounded.abs() < 1e-6 {
        format!("{rounded:e}")
    } else {
        format!("{rounded}")
    }
}

fn texture_name(texture: &Option<TextureRef>) -> &str {
    match texture.as_ref().map(|t| &t.source) {
        None => "-",
        Some(TextureSource::File(path)) => path,
        Some(TextureSource::Generated(_)) => "canvas",
    }
}

/// The Three material fields the dump compares. Effects that moved a texture into
/// `map` for the renderer had none in Three (they sampled it in TSL).
fn material_summary(m: &Material) -> String {
    let effect_map = matches!(&m.effect, Effect::Custom { name, .. }
        if [WATER, QUARRY_SOIL, SAND_DRIFT].contains(name));
    [
        if m.shading == Shading::Basic {
            "basic".into()
        } else {
            "standard".to_string()
        },
        format!("{:06x}", m.color.0),
        format!("{:.4}", f64::from(m.roughness)),
        format!("{:.4}", f64::from(m.metalness)),
        if effect_map {
            "-".into()
        } else {
            texture_name(&m.map).to_string()
        },
        texture_name(&m.bump_map).to_string(),
        format!("{:.4}", f64::from(m.bump_scale)),
        u8::from(m.vertex_colors).to_string(),
        u8::from(m.flat_shading).to_string(),
        u8::from(m.transparent).to_string(),
        format!("{:.3}", f64::from(m.opacity)),
        u8::from(m.depth_write).to_string(),
        (m.side as u8).to_string(),
        u8::from(m.tone_mapped).to_string(),
        u8::from(m.fog).to_string(),
        format!("{:06x}", m.emissive.0),
        format!("{:.3}", f64::from(m.emissive_intensity)),
    ]
    .join(",")
}

/// A short stable hash of a material summary, keeping the embedded dump small.
fn summary_hash(summary: &str) -> String {
    format!("{:08x}", fnv(summary.bytes().map(u32::from)))
}

fn dump(node: &Node, path: &str, full: bool, lines: &mut Vec<String>) {
    let q = node.rotation;
    let transform = [
        [node.position.x, node.position.y, node.position.z]
            .map(num)
            .join(","),
        [q.x, q.y, q.z, q.w].map(num).join(","),
        [node.scale.x, node.scale.y, node.scale.z]
            .map(num)
            .join(","),
    ]
    .join("|");
    let mut line = format!(
        "{path}|{}|{}|{}|{}",
        if node.name.is_empty() {
            "-"
        } else {
            &node.name
        },
        u8::from(node.visible),
        node.children.len(),
        if transform == "0,0,0|0,0,0,1|1,1,1" {
            "="
        } else {
            &transform
        },
    );
    if let Some(drawable) = &node.drawable {
        let mesh = &drawable.mesh;
        let hash_or_dash = |empty: bool, hash: String| if empty { "-".to_string() } else { hash };
        let alpha = mesh.attribute(VERTEX_ALPHA);
        let mut extra: Vec<_> = mesh
            .attributes
            .iter()
            .filter(|a| a.name != VERTEX_ALPHA)
            .map(|a| format!("{}:{}:{}", a.name, a.item_size, float_hash(&a.data)))
            .collect();
        extra.sort();
        let summary = material_summary(&drawable.material);
        write!(
            line,
            "|V={}|I={}|P={}|N={}|U={}|C={}|A={}|X={}|M={}|F={},{},{},{}",
            mesh.vertex_count(),
            mesh.indices.as_ref().map_or(-1, |i| i.len() as i64),
            float_hash(mesh.positions.iter().flatten()),
            hash_or_dash(
                mesh.normals.is_empty(),
                float_hash(mesh.normals.iter().flatten())
            ),
            hash_or_dash(mesh.uvs.is_empty(), float_hash(mesh.uvs.iter().flatten())),
            hash_or_dash(
                mesh.colors.is_empty(),
                float_hash(mesh.colors.iter().flatten())
            ),
            alpha.map_or("-".into(), |a| float_hash(&a.data)),
            if extra.is_empty() {
                "-".into()
            } else {
                extra.join(";")
            },
            if full {
                summary.clone()
            } else {
                summary_hash(&summary)
            },
            u8::from(drawable.cast_shadow),
            u8::from(drawable.receive_shadow),
            drawable.render_order,
            u8::from(drawable.frustum_culled),
        )
        .unwrap();
        if let Some(instances) = &drawable.instances {
            let smoke = matches!(&drawable.material.effect, Effect::Custom { name, .. } if *name == CHIMNEY_SMOKE);
            if smoke {
                write!(line, "|INST={},-,-", instances.len()).unwrap();
            } else {
                let matrices: Vec<f32> = instances
                    .iter()
                    .flat_map(|i| i.matrix.to_cols_array().map(|v| v as f32))
                    .collect();
                let colors: Vec<f32> = instances
                    .iter()
                    .flat_map(|i| i.color.unwrap_or_default())
                    .collect();
                write!(
                    line,
                    "|INST={},{},{}",
                    instances.len(),
                    float_hash(&matrices),
                    float_hash(&colors)
                )
                .unwrap();
            }
        }
    }
    lines.push(line);
    for (i, child) in node.children.iter().enumerate() {
        dump(child, &format!("{path}.{i}"), full, lines);
    }
}

fn sections() -> Vec<(String, Node)> {
    let mut sections = Vec::new();
    let village = VillageScenery::new();
    sections.push(("village".to_string(), village.root.clone()));
    let mut village2 = VillageScenery::new();
    village2.set_covers(&[
        SmokeCover {
            kind: "house",
            destructible: false,
            x: 10.0,
            z: 20.0,
            w: 8.0,
            h: 5.0,
            d: 6.0,
        },
        SmokeCover {
            kind: "house",
            destructible: true,
            x: 30.0,
            z: 20.0,
            w: 8.0,
            h: 5.0,
            d: 6.0,
        },
        SmokeCover {
            kind: "tree",
            destructible: false,
            x: 0.0,
            z: 0.0,
            w: 1.0,
            h: 5.0,
            d: 1.0,
        },
    ]);
    village2.update(3.7);
    sections.push((
        "village-smoke".into(),
        village2.root.find(CHIMNEY_SMOKE_NODE).unwrap().clone(),
    ));
    sections.push((
        "waterwheel".into(),
        village2.root.find(WATERWHEEL).unwrap().clone(),
    ));
    let mut harbor = HarborScenery::new();
    sections.push(("harbor".into(), harbor.root.clone()));
    harbor.update(5.3);
    sections.push(("harbor-5.3".into(), harbor.root.children[1].clone()));
    sections.push(("quarry".into(), quarry_scenery()));
    for (name, kind, extent, y) in [
        ("dry-grass", GroundKind::DryGrass, 120.0, 0.008),
        ("packed-dirt", GroundKind::PackedDirt, 140.0, -0.002),
        ("packed-dirt", GroundKind::PackedDirt, 78.0, 0.008),
        ("dry-grass", GroundKind::DryGrass, 140.0, -0.002),
    ] {
        sections.push((
            format!("floor-{name}-{extent}"),
            create_arena_floor(kind, extent, y),
        ));
    }
    sections.push(("pads-0.65".into(), create_spawn_pads(0.65)));
    for c in [
        CargoShape {
            w: 6.0,
            d: 14.0,
            h: 3.6,
            color: 0xd37c38,
        },
        CargoShape {
            w: 12.0,
            d: 5.0,
            h: 3.6,
            color: 0x6689ad,
        },
    ] {
        let mut group = Node::group("");
        shipping_container(&mut group, c);
        sections.push((format!("container-{}", c.w), group));
    }
    sections.push(("footing".into(), sandstone_footing(14.0, 8.0, 37)));
    sections
}

/// Subtrees only one side builds: Three's reflector targets.
fn skipped(section: &str, path: &str) -> bool {
    (section == "village" && path.starts_with("r.4.1."))
        || (section == "harbor" && path.starts_with("r.0."))
}

/// Lines whose child count differs only because of a skipped subtree.
fn loose_children(section: &str, path: &str) -> bool {
    (section == "village" && path == "r.4.1") || (section == "harbor" && path == "r.0")
}

/// The ships' painted batches, which carry their cargo (fleet child `ship.1`).
fn square_ship_fittings(section: &str, path: &str) -> bool {
    let ships = ["0", "1", "2"];
    match section {
        "harbor" => ships.iter().any(|ship| path == format!("r.1.{ship}.1")),
        "harbor-5.3" => ships.iter().any(|ship| path == format!("r.{ship}.1")),
        _ => false,
    }
}

fn without_geometry(line: &str) -> String {
    line.split('|')
        .map(|field| match field.split_once('=') {
            Some((key @ ("V" | "I" | "P" | "N" | "U" | "C"), _)) => format!("{key}=*"),
            _ => field.to_string(),
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Forest batches whose normals are compared on a grid (see the module docs).
fn grid_normals(section: &str, path: &str) -> bool {
    section == "village" && path.starts_with("r.4.4.")
}

fn without_normals(line: &str) -> String {
    let mut fields: Vec<&str> = line.split('|').collect();
    if let Some(field) = fields.iter_mut().find(|f| f.starts_with("N=")) {
        *field = "N=*";
    }
    fields.join("|")
}

fn without_children(line: &str) -> String {
    let mut fields: Vec<&str> = line.split('|').collect();
    fields[3] = "*";
    fields.join("|")
}

#[test]
fn scenery_matches_typescript() {
    let full = std::env::var("SCENERY_DUMP").ok();
    let mut actual = Vec::new();
    let mut dumped = Vec::new();
    for (name, root) in sections() {
        let mut lines = Vec::new();
        dump(&root, "r", false, &mut lines);
        actual.push((name.clone(), lines));
        if full.is_some() {
            dumped.push(format!("# {name}"));
            dump(&root, "r", true, &mut dumped);
        }
    }
    if let Some(file) = &full {
        std::fs::write(file, dumped.join("\n") + "\n").unwrap();
    }
    let mut expected: Vec<(String, Vec<String>)> = Vec::new();
    for line in EXPECTED.lines() {
        if let Some(name) = line.strip_prefix("# ") {
            expected.push((name.to_string(), Vec::new()));
        } else if let Some((_, lines)) = expected.last_mut() {
            lines.push(line.to_string());
        }
    }
    let mut mismatches = Vec::new();
    for (name, lines) in &actual {
        let Some((_, reference)) = expected.iter().find(|(n, _)| n == name) else {
            panic!("no reference for {name}");
        };
        let keep = |line: &&String| !skipped(name, line.split('|').next().unwrap());
        let ours: Vec<&String> = lines.iter().filter(keep).collect();
        let theirs: Vec<&String> = reference.iter().filter(keep).collect();
        for (a, b) in ours.iter().zip(&theirs) {
            let path = a.split('|').next().unwrap();
            let (a, b) = if loose_children(name, path) {
                (without_children(a), without_children(b))
            } else if grid_normals(name, path) {
                (without_normals(a), without_normals(b))
            } else if square_ship_fittings(name, path) {
                (without_geometry(a), without_geometry(b))
            } else {
                (a.to_string(), b.to_string())
            };
            if a != b {
                mismatches.push(format!("{name}\n  rust: {a}\n  ts:   {b}"));
            }
        }
        if ours.len() != theirs.len() {
            mismatches.push(format!("{name}: {} nodes vs {}", ours.len(), theirs.len()));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// Normals of the background forest batches on a 1/65536 grid, printed by the
/// same Node script (`Math.round(n * 65536) >>> 0`, FNV-1a).
const FOREST_NORMALS: [u32; 8] = [
    0xf598fba9, 0xfba8767a, 0xc929b2f3, 0xd55434e8, 0xe17bc1d0, 0x2636dec2, 0x7d7e3200, 0xe0290598,
];

#[test]
fn forest_normals_match_typescript_on_a_grid() {
    let village = VillageScenery::new();
    let forest = &village.root.children[4].children[4];
    let hashes: Vec<u32> =
        forest
            .children
            .iter()
            .map(|batch| {
                let mesh = &batch.drawable.as_ref().expect("a batch").mesh;
                fnv(mesh.normals.iter().flatten().map(|&n| {
                    crate::geometry::math::js_round(f64::from(n) * 65536.0) as i64 as u32
                }))
            })
            .collect();
    assert_eq!(hashes, FOREST_NORMALS);
}

/// A `key|hash` line of the reference's `soil` section.
fn soil_hash(key: &str) -> String {
    EXPECTED
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}|")).map(str::to_string))
        .unwrap_or_else(|| panic!("no soil reference {key}"))
}

fn byte_hash(pixels: &[u8]) -> String {
    format!(
        "{:08x}",
        fnv(pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word)))
    )
}

#[test]
fn soil_bake_matches_typescript() {
    assert_eq!(float_hash(sand_accum()), soil_hash("accum"));
    // A later band first replays the earlier rows' draws.
    let accum = sand_accum();
    assert_eq!(
        byte_hash(&bake_quarry_soil(accum, 1000, 1003)),
        soil_hash("band-1000-1003")
    );
    assert_eq!(
        byte_hash(&bake_quarry_soil(accum, 0, 2)),
        soil_hash("band-0-2")
    );
}

/// The full 2048² bake and each theme's build time. Run in release:
/// `cargo test -p sloppy-core --release scenery_build_times -- --ignored --nocapture`.
#[test]
#[ignore = "slow in debug; run in release for timings"]
fn scenery_build_times() {
    use std::time::Instant;
    let start = Instant::now();
    let soil = bake_quarry_soil(sand_accum(), 0, QUARRY_SOIL_SIZE);
    let bake = start.elapsed();
    assert_eq!(byte_hash(&soil), soil_hash("full"));
    println!("quarry soil bake: {:.1} ms", bake.as_secs_f64() * 1e3);
    for theme in [MapTheme::Village, MapTheme::Harbor, MapTheme::Quarry] {
        let start = Instant::now();
        let scenery = build_scenery(theme);
        let elapsed = start.elapsed();
        let mut meshes = 0;
        scenery.root().traverse(DMat4::IDENTITY, &mut |node, _| {
            meshes += usize::from(node.drawable.is_some());
        });
        println!(
            "{theme:?}: {:.1} ms ({meshes} drawables)",
            elapsed.as_secs_f64() * 1e3
        );
    }
    let start = Instant::now();
    let floors = [
        create_arena_floor(GroundKind::DryGrass, 120.0, 0.008),
        create_arena_floor(GroundKind::PackedDirt, 140.0, -0.002),
    ];
    let pads = create_spawn_pads(0.65);
    println!(
        "extra-level floors and pads: {:.1} ms ({} floors, {} pad batches)",
        start.elapsed().as_secs_f64() * 1e3,
        floors.len(),
        pads.children.len()
    );
}

#[test]
fn scenery_textures_per_theme() {
    let names = |theme| -> Vec<String> {
        node_textures(build_scenery(theme).root())
            .into_iter()
            .map(|source| match source {
                TextureSource::File(path) => path.to_string(),
                TextureSource::Generated(key) => format!("generated:{key}"),
            })
            .collect()
    };
    assert_eq!(
        names(MapTheme::Harbor),
        [
            "textures/water/normals.webp",
            "textures/harbor/steel.webp",
            "textures/harbor/dock.webp",
            "generated:harbor-label-b2",
            "generated:harbor-label-b1",
            "generated:harbor-label-harbor-havoc",
            "generated:harbor-label-port-07",
            "generated:harbor-label-loading",
            "textures/houses/siding.webp",
        ]
    );
    assert_eq!(
        names(MapTheme::Quarry),
        [
            "generated:quarry-soil",
            "textures/ground/packed-dirt.webp",
            "textures/harbor/steel.webp",
            "textures/tanks/armor-wear.webp",
            "textures/quarry/sandstone.webp",
            "textures/walls/weathered-concrete.webp",
            "generated:quarry-sign",
        ]
    );
    // The village also draws the tree models' textures.
    let village = names(MapTheme::Village);
    for texture in [
        "textures/ground/dry-grass.webp",
        "textures/ground/packed-dirt.webp",
        "textures/water/normals.webp",
        "textures/houses/siding.webp",
        "textures/houses/shingles.webp",
        "textures/walls/weathered-concrete.webp",
        "generated:village-sign",
    ] {
        assert!(village.iter().any(|t| t == texture), "{texture}");
    }
    for key in [
        effects_scenery::VILLAGE_SIGN_TEXTURE,
        effects_scenery::QUARRY_SIGN_TEXTURE,
        "harbor-label-loading",
    ] {
        assert!(effects_scenery::canvas_texture(key).is_some(), "{key}");
    }
}

fn triangles(node: &Node) -> usize {
    let own = node.drawable.as_ref().map_or(0, |drawable| {
        let mesh = &drawable.mesh;
        mesh.indices.as_ref().map_or(mesh.vertex_count(), Vec::len) / 3
    });
    own + node.children.iter().map(triangles).sum::<usize>()
}

#[test]
fn ship_cargo_uses_square_fittings() {
    let shape = CargoShape {
        w: 9.6,
        d: 4.5,
        h: 3.2,
        color: 0xcb7d43,
    };
    let built = |build: fn(&mut Node, CargoShape)| {
        let mut group = Node::group("");
        build(&mut group, shape);
        triangles(&group)
    };
    let cover = built(shipping_container);
    let cargo = built(harbor_models::ship_container);
    assert!(
        cargo * 5 < cover,
        "cargo {cargo} vs cover {cover} triangles"
    );
    // Each ship is drawn for the view, the sun's shadow and the water reflection.
    let harbor = HarborScenery::new();
    for ship in &harbor.root.children[1].children[..3] {
        assert!(triangles(ship) < 25_000, "{} triangles", triangles(ship));
    }
}

#[test]
fn animated_scenery_parts() {
    let mut village = VillageScenery::new();
    village.update(2.0);
    let wheel = village.root.find(WATERWHEEL).unwrap();
    assert!((wheel.rotation.z - (-0.16f64).sin()).abs() < 1e-12);
    let mut harbor = HarborScenery::new();
    let beacons = |harbor: &HarborScenery| harbor.root.children.last().unwrap().visible;
    assert!(beacons(&harbor));
    // sin(2.5 * 1.6) = sin(4) < -0.3: the beacons blink off.
    harbor.update(1.6);
    assert!(!beacons(&harbor));
}

#[test]
fn quarry_rock_shape_and_spawn_pads() {
    let shape = quarry_rock_shape(2.0, 1.0, 3.0, 5);
    assert_eq!(shape.positions.len(), (7 * 16 + 1) * 3);
    assert_eq!(shape.indices.len(), (6 * 16 * 2 + 16) * 3);
    let pieces = quarry_spawn_pad_pieces(Team::Red);
    assert_eq!(pieces.len(), 18);
    assert!(pieces.iter().all(|p| p.y + p.h / 2.0 < 1.2));
}

/// Material and attribute features Three expressed directly, which the scene
/// contract carries instead of UV tricks or effect parameters.
#[test]
fn materials_carry_three_features() {
    let house = house_texture(HouseSurface::Siding);
    assert!(!house.flip_y);
    assert_eq!((house.repeat, house.offset), ([1.0, 1.0], [0.0, 0.0]));
    assert_eq!(concrete_material().shadow_side, Some(Side::Front));
    let drift = sand_drift_material();
    assert_eq!(drift.polygon_offset, Some((-1.0, -1.0)));
    assert_eq!(drift.extra_textures[0].0, effects_scenery::GRIT);
    assert_eq!(
        sandstone_material().extra_textures[0].0,
        effects_scenery::SOIL
    );
    let ammo = pickup_cube(crate::sim::types::PickupKind::Rocket);
    let face = ammo.children[0].drawable.as_ref().unwrap();
    assert!(face.material.emissive_map.is_some());
    assert_eq!(face.material.emissive_map, face.material.map);
    let village = VillageScenery::new();
    let smoke = village.root.find(CHIMNEY_SMOKE_NODE).unwrap();
    let mesh = &smoke.drawable.as_ref().unwrap().mesh;
    assert!(mesh.attributes.iter().all(|a| a.per_instance));
    let footing = sandstone_footing(14.0, 8.0, 37);
    let alpha = footing.drawable.as_ref().unwrap().mesh.vertex_alpha();
    assert_eq!(alpha.map(<[f32]>::len), Some(48));
}

/// Printed by the reference script (see the module docs), compacted.
#[rustfmt::skip]
const EXPECTED: &str = r#"
# village
r|pine-village-scenery|1|9|=
r.0|-|1|0|0,-0.8,0|0,0,0,1|1,1,1|V=324|I=-1|P=e793c4f9|N=04279eb5|U=29c6023d|C=c0134d31|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1|-|1|0|0,0.008,0|0,0,0,1|1,1,1|V=2401|I=13824|P=da6a05a7|N=6b6c5f6d|U=39dcbecd|C=7004e728|A=-|X=-|M=d9006d5a|F=0,1,0,1
r.2|-|1|1|=
r.2.0|-|1|0|=|V=324|I=-1|P=fc34780b|N=0304f5b5|U=55914565|C=b104f5b5|A=8c200615|X=-|M=cfd9e356|F=0,1,-1,1
r.3|-|1|3|=
r.3.0|spawn-pads|1|4|=
r.3.0.0|-|1|0|=|V=14400|I=-1|P=baa5d0e5|N=9de8fa65|U=a13b5035|C=008df045|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.3.0.1|-|1|0|=|V=4620|I=-1|P=d05dcf47|N=68402dbd|U=f9864bd5|C=69c9f195|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.3.0.2|-|1|0|=|V=32400|I=-1|P=06cd4685|N=8e0ad905|U=c4155b65|C=ce6e67c5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.3.0.3|-|1|0|=|V=4620|I=-1|P=9fa5f5e7|N=c84f2335|U=f9864bd5|C=4366ccc1|A=-|X=-|M=d33b7c4a|F=1,1,0,1
r.3.1|-|1|0|=|V=37584|I=-1|P=ba8f569d|N=7541dc05|U=a9362ec5|C=46caf485|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.3.2|-|1|0|=|V=37584|I=-1|P=731dfa3d|N=7541dc05|U=a9362ec5|C=34b889f5|A=-|X=-|M=d33b7c4a|F=1,1,0,1
r.4|pine-valley-landscape|1|5|=
r.4.0|-|1|0|=|V=12769|I=75264|P=e64f13b9|N=3f8186ec|U=365fd62d|C=0a5a31ee|A=-|X=-|M=d9006d5a|F=0,1,0,1
r.4.1|village-creek|1|1|0,-2.65,0|-0.707106781,0,0,0.707106781|1,1,1|V=322|I=960|P=0884e83d|N=1b97729d|U=be71afd5|C=-|A=-|X=-|M=d4dd116a|F=0,0,0,1
r.4.1.0|-|1|0|=
r.4.2|pine-mountain-ridge|1|0|=|V=2457|I=14040|P=8eda18c3|N=d77fd824|U=4a0d61cb|C=642c7cec|A=-|X=-|M=851883b1|F=0,0,0,1
r.4.3|-|1|1|=
r.4.3.0|-|1|0|=|V=8400|I=-1|P=72061a56|N=14a64e51|U=99ef87a5|C=43c19091|A=-|X=-|M=4083e32b|F=1,1,0,1
r.4.4|-|1|8|=
r.4.4.0|-|1|0|=|V=3552|I=-1|P=68aa23ae|N=40849877|U=423180c5|C=e4f0b505|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.4.1|-|1|0|=|V=23940|I=-1|P=449b8ada|N=cdf944c5|U=4381ed65|C=44072bb1|A=-|X=-|M=2be7cafb|F=1,1,0,1
r.4.4.2|-|1|0|=|V=6828|I=-1|P=8ff3cba5|N=25afdf19|U=1672d8b5|C=de7c3805|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.4.3|-|1|0|=|V=28980|I=-1|P=8c318c61|N=56fe8059|U=9b63dce5|C=0c32cf85|A=-|X=-|M=dd529ec4|F=1,1,0,1
r.4.4.4|-|1|0|=|V=30960|I=-1|P=02962604|N=391bd473|U=7c1abf45|C=3ea1a48f|A=-|X=-|M=c21f48a0|F=1,1,0,1
r.4.4.5|-|1|0|=|V=29412|I=-1|P=0d67c0d3|N=a6659d4a|U=c54bbc65|C=8fc5df7b|A=-|X=-|M=5e58daa5|F=1,1,0,1
r.4.4.6|-|1|0|=|V=24768|I=-1|P=b031bd7b|N=319eab17|U=e2c96bc5|C=937f9723|A=-|X=-|M=ff64dbb2|F=1,1,0,1
r.4.4.7|-|1|0|=|V=22680|I=-1|P=311f51e8|N=ff7eae7e|U=d89df385|C=c7350f5d|A=-|X=-|M=d2f282e8|F=1,1,0,1
r.5|village-meadow|1|2|=
r.5.0|-|1|0|=|V=12|I=-1|P=c84148df|N=8f2c550b|U=-|C=-|A=-|X=windOrigin:2:b0e57cfa|M=543eda4f|F=0,1,0,1|INST=1948,3337f077,819444d3
r.5.1|-|1|0|=|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=23d1a285|F=0,1,0,1|INST=601,116d27e5,c5dc1e08
r.6|village-chimney-smoke|1|0|=|V=4|I=6|P=463c62b5|N=9d3c62b5|U=d6e17165|C=-|A=-|X=phase:1:a6f6753d;smokeOrigin:3:20e0d2c5|M=f5df665b|F=0,0,0,0|INST=0,-,-
r.7|-|1|4|=
r.7.0|pine-watermill|1|2|-35,-0.25,-69|0,0,0,1|1,1,1
r.7.0.0|-|1|5|=
r.7.0.0.0|-|1|0|=|V=2916|I=-1|P=bbfe227d|N=07df6f35|U=c45ce495|C=6cd04935|A=-|X=-|M=ec422f06|F=1,1,0,1
r.7.0.0.1|-|1|0|=|V=564|I=-1|P=0c12c179|N=f3c53715|U=bf0a44e5|C=688fcfcd|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.0.0.2|-|1|0|=|V=36|I=-1|P=c3f7eebf|N=e2bc5439|U=8bc9ae45|C=2fe06845|A=-|X=-|M=839d0a3e|F=1,1,0,1
r.7.0.0.3|-|1|0|=|V=540|I=-1|P=0cd2bc3b|N=e4e64755|U=2d081b25|C=18a110ed|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.0.0.4|-|1|0|=|V=120|I=-1|P=0b7b18fd|N=c07e01ab|U=3dd9bc15|C=8ec68e45|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.0.1|turning-waterwheel|1|4|-8,1.1,-13.6|0,0,0,1|1,1,1
r.7.0.1.0|-|1|0|=|V=3456|I=-1|P=4df199d5|N=004a3eb5|U=e4e1b7a5|C=8554adc5|A=-|X=-|M=dc999f91|F=1,1,0,1
r.7.0.1.1|-|1|0|=|V=864|I=-1|P=3002ceed|N=0285bb85|U=9f583cc5|C=612550a5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.0.1.2|-|1|0|=|V=720|I=-1|P=d8d61cd5|N=e2cda325|U=12948245|C=00040625|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.0.1.3|-|1|0|=|V=144|I=-1|P=67f7205f|N=a782a24b|U=6d704989|C=865e1a55|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.1|village-timber-bridge|1|3|0,0,-82|0,0,0,1|1,1,1
r.7.1.0|-|1|0|=|V=1116|I=-1|P=5eb9e741|N=c9a95785|U=a6284525|C=4d9ba871|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.1.1|-|1|0|=|V=2304|I=-1|P=069c8835|N=c16a2445|U=964a45c5|C=1360caed|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.1.2|-|1|0|=|V=648|I=-1|P=fff41961|N=f1cbd625|U=80872625|C=ce775aa5|A=-|X=-|M=ec422f06|F=1,1,0,1
r.7.2|village-timber-bridge|1|3|-85,0,38|0,0.707106781,0,0.707106781|1,1,1
r.7.2.0|-|1|0|=|V=1116|I=-1|P=5eb9e741|N=c9a95785|U=a6284525|C=4d9ba871|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.2.1|-|1|0|=|V=2304|I=-1|P=069c8835|N=c16a2445|U=964a45c5|C=1360caed|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.2.2|-|1|0|=|V=648|I=-1|P=fff41961|N=f1cbd625|U=80872625|C=ce775aa5|A=-|X=-|M=ec422f06|F=1,1,0,1
r.7.3|village-log-cart|1|3|-68,-0.65,-24|0,0.0848976828,0,0.996389675|1,1,1
r.7.3.0|-|1|0|=|V=36|I=-1|P=a3137221|N=f87a9e35|U=c0efde65|C=c7066fc5|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.3.1|-|1|0|=|V=3492|I=-1|P=f8dcb593|N=15d07677|U=1efa79f9|C=f9352601|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.3.2|-|1|0|=|V=72|I=-1|P=9b32b929|N=f197b4d5|U=155c3305|C=4f70298d|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.8|-|1|2|=
r.8.0|-|1|0|=|V=2052|I=-1|P=8ee44b4f|N=3b68aea1|U=df7ddf85|C=4c16bbe1|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.8.1|-|1|0|=|V=36|I=-1|P=5a1045b9|N=f87a9e35|U=5aefde65|C=e27a9e35|A=-|X=-|M=05369b81|F=1,1,0,1
# village-smoke
r|village-chimney-smoke|1|0|=|V=4|I=6|P=463c62b5|N=9d3c62b5|U=d6e17165|C=-|A=-|X=phase:1:a6f6753d;smokeOrigin:3:ad44e6c5|M=f5df665b|F=0,0,0,0|INST=16,-,-
# waterwheel
r|turning-waterwheel|1|4|-8,1.1,-13.6|0,0,-0.291696507,0.956510924|1,1,1
r.0|-|1|0|=|V=3456|I=-1|P=4df199d5|N=004a3eb5|U=e4e1b7a5|C=8554adc5|A=-|X=-|M=dc999f91|F=1,1,0,1
r.1|-|1|0|=|V=864|I=-1|P=3002ceed|N=0285bb85|U=9f583cc5|C=612550a5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.2|-|1|0|=|V=720|I=-1|P=d8d61cd5|N=e2cda325|U=12948245|C=00040625|A=-|X=-|M=98e0a408|F=1,1,0,1
r.3|-|1|0|=|V=144|I=-1|P=67f7205f|N=a782a24b|U=6d704989|C=865e1a55|A=-|X=-|M=c3d0892e|F=1,1,0,1
# harbor
r|-|1|15|=
r.0|harbor-water|1|1|0,-2.2,0|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=b35862b5|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=d4dd116a|F=0,0,0,1
r.0.0|-|1|0|=
r.1|-|1|7|=
r.1.0|container-ship|1|3|-8,0,-80|0,0,0,1|1,1,1
r.1.0.0|-|1|0|=|V=798|I=-1|P=6f3cc37d|N=b4337401|U=099cbcd5|C=ec994e19|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.0.1|-|1|0|=|V=392004|I=-1|P=a8349987|N=cc3e96c5|U=2e418e8d|C=92fb31f5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.0.2|-|1|0|=|V=1008|I=-1|P=5b13d4d1|N=deefd363|U=62586281|C=6fbd9c25|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.1|container-ship|1|3|-77,0.0636508199,-8|0.000535508804,0.707106578,0.000535508804,0.707106578|0.85,0.85,0.85
r.1.1.0|-|1|0|=|V=798|I=-1|P=cde6e825|N=b4337401|U=099cbcd5|C=b6a69259|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.1.1|-|1|0|=|V=392004|I=-1|P=d35dfef7|N=cc3e96c5|U=2e418e8d|C=7151f081|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.1.2|-|1|0|=|V=1008|I=-1|P=5b13d4d1|N=deefd363|U=62586281|C=6fbd9c25|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.2|container-ship|1|3|77,-0.0529761747,15|0.000578673274,-0.707106544,-0.000578673274,0.707106544|0.72,0.72,0.72
r.1.2.0|-|1|0|=|V=798|I=-1|P=e659980d|N=b4337401|U=099cbcd5|C=7c90c6a9|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.2.1|-|1|0|=|V=392004|I=-1|P=f637ac5f|N=cc3e96c5|U=2e418e8d|C=b4c1f1e5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.2.2|-|1|0|=|V=1008|I=-1|P=5b13d4d1|N=deefd363|U=62586281|C=6fbd9c25|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.3|quay-crane|1|5|-40,0,-65|0,0,0,1|1,1,1
r.1.3.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.1.3.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.3.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.3.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.3.4|-|1|2|0,18,-15|0,0,0,1|1,1,1
r.1.3.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.3.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.4|quay-crane|1|5|40,0,-65|0,0,0,1|1,1,1
r.1.4.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.1.4.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.4.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.4.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.4.4|-|1|2|0,18,-15|0,0,0.0105181934,0.999944682|1,1,1
r.1.4.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.4.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.5|quay-crane|1|5|-65.5,0,16|0,0.707106781,0,0.707106781|0.75,0.75,0.75
r.1.5.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.1.5.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.5.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.5.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.5.4|-|1|2|0,18,-15|0,0,0.0113659731,0.999935405|1,1,1
r.1.5.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.5.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.6|quay-crane|1|5|65.5,0,-20|0,-0.707106781,0,0.707106781|0.75,0.75,0.75
r.1.6.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.1.6.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.6.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.6.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.6.4|-|1|2|0,18,-15|0,0,0.00176399919,0.999998444|1,1,1
r.1.6.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.6.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.2|-|1|0|-37,0.035,-32|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.3|-|1|0|-37,0.035,-12|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.4|-|1|0|-37,0.035,12|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.5|-|1|0|-37,0.035,32|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.6|-|1|0|37,0.035,-32|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.7|-|1|0|37,0.035,-12|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.8|-|1|0|37,0.035,12|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.9|-|1|0|37,0.035,32|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=7e142311|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.10|-|1|0|0,0.035,-48|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=877c62b5|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.11|-|1|0|0,0.035,48|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=509c62b5|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.12|-|1|0|0,0.035,3.5|-0.707106781,0,0,0.707106781|1,1,1|V=4|I=6|P=6b516bd9|N=9d3c62b5|U=d6e17165|C=-|A=-|X=-|M=ba62c4d4|F=0,0,0,1
r.13|-|1|6|=
r.13.0|-|1|0|=|V=432|I=-1|P=8cdcd42d|N=faac1d05|U=52ef32d5|C=819d5579|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.13.1|-|1|0|=|V=25776|I=-1|P=eec684d7|N=dea3e245|U=a2f8ca65|C=405b7e05|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.13.2|-|1|0|=|V=6048|I=-1|P=a3e55365|N=387e23c5|U=1615478d|C=c50951a5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.13.3|-|1|0|=|V=960|I=-1|P=05eaae9f|N=7e63a6c5|U=a08bdc87|C=4994bec5|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.13.4|-|1|0|=|V=960|I=-1|P=77f190a3|N=7e63a6c5|U=a08bdc87|C=fbd88305|A=-|X=-|M=d33b7c4a|F=1,1,0,1
r.13.5|-|1|0|=|V=1512|I=-1|P=1d60380d|N=b3c89925|U=abd57c05|C=0fe13265|A=-|X=-|M=98e0a408|F=1,1,0,1
r.14|-|1|1|=
r.14.0|-|1|0|=|V=384|I=-1|P=e08c8205|N=23412b65|U=bc0999c5|C=c34f3a45|A=-|X=-|M=c3d0892e|F=1,1,0,1
# harbor-5.3
r|-|1|7|=
r.0|container-ship|1|3|-8,0.0157261011,-80|0.000713631397,0,0,0.999999745|1,1,1
r.0.0|-|1|0|=|V=798|I=-1|P=6f3cc37d|N=b4337401|U=099cbcd5|C=ec994e19|A=-|X=-|M=90c71f94|F=1,1,0,1
r.0.1|-|1|0|=|V=392004|I=-1|P=a8349987|N=cc3e96c5|U=2e418e8d|C=92fb31f5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.0.2|-|1|0|=|V=1008|I=-1|P=5b13d4d1|N=deefd363|U=62586281|C=6fbd9c25|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1|container-ship|1|3|-77,-0.0685681156,-8|-0.0000536527442,0.707106779,-0.0000536527442,0.707106779|0.85,0.85,0.85
r.1.0|-|1|0|=|V=798|I=-1|P=cde6e825|N=b4337401|U=099cbcd5|C=b6a69259|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.1|-|1|0|=|V=392004|I=-1|P=d35dfef7|N=cc3e96c5|U=2e418e8d|C=7151f081|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.2|-|1|0|=|V=1008|I=-1|P=5b13d4d1|N=deefd363|U=62586281|C=6fbd9c25|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.2|container-ship|1|3|77,0.0413427076,15|-0.000562590987,-0.707106557,0.000562590987,0.707106557|0.72,0.72,0.72
r.2.0|-|1|0|=|V=798|I=-1|P=e659980d|N=b4337401|U=099cbcd5|C=7c90c6a9|A=-|X=-|M=90c71f94|F=1,1,0,1
r.2.1|-|1|0|=|V=392004|I=-1|P=f637ac5f|N=cc3e96c5|U=2e418e8d|C=b4c1f1e5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.2.2|-|1|0|=|V=1008|I=-1|P=5b13d4d1|N=deefd363|U=62586281|C=6fbd9c25|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.3|quay-crane|1|5|-40,0,-65|0,0,0,1|1,1,1
r.3.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.3.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.3.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.3.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.3.4|-|1|2|0,18,-15|0,0,-0.00672858058,0.999977363|1,1,1
r.3.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.3.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.4|quay-crane|1|5|40,0,-65|0,0,0,1|1,1,1
r.4.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.4.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.4.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.4.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.4.4|-|1|2|0,18,-15|0,0,-0.0124996388,0.999921876|1,1,1
r.4.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.4.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.5|quay-crane|1|5|-65.5,0,16|0,0.707106781,0,0.707106781|0.75,0.75,0.75
r.5.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.5.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.5.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.5.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.5.4|-|1|2|0,18,-15|0,0,-0.00677883583,0.999977023|1,1,1
r.5.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.5.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
r.6|quay-crane|1|5|65.5,0,-20|0,-0.707106781,0,0.707106781|0.75,0.75,0.75
r.6.0|-|1|0|=|V=36|I=-1|P=2fca04b7|N=f87a9e35|U=9824ca75|C=7a64e385|A=-|X=-|M=e47b77c2|F=1,1,0,1
r.6.1|-|1|0|=|V=180|I=-1|P=35d5247b|N=d1e3e1f5|U=88456fed|C=16cc6f2d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.6.2|-|1|0|=|V=576|I=-1|P=e93a571d|N=8c525755|U=91ee01b5|C=a67897c5|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.6.3|-|1|0|=|V=3204|I=-1|P=f32a3442|N=93ae81c5|U=89520565|C=68cd6245|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.6.4|-|1|2|0,18,-15|0,0,0.00517464387,0.999986611|1,1,1
r.6.4.0|-|1|0|=|V=720|I=-1|P=22eb16f5|N=5d676e85|U=7a056195|C=78253005|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.6.4.1|-|1|0|=|V=36|I=-1|P=3fc37689|N=f87a9e35|U=788dcaa5|C=5a889749|A=-|X=-|M=90c71f94|F=1,1,0,1
# quarry
r|dusty-dig-scenery|1|6|=
r.0|quarry-compacted-haul-roads|1|0|0,0.008,0|0,0,0,1|1,1,1|V=19881|I=117600|P=0f9c9537|N=09c9410b|U=6a1a4b4d|C=8bc7cfd7|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.1|quarry-excavator|1|8|-24,-1.75,-68|0,-0.149438132,0,0.988771078|1,1,1
r.1.0|-|1|1|11.5,0.6,0.9|0,0,-0.109778301,0.993956098|1,1,1
r.1.0.0|-|1|0|=|V=324|I=-1|P=f4ced8ff|N=d704f5b5|U=575f872d|C=6d5f3d95|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.1|-|1|0|=|V=1476|I=-1|P=3c9849f9|N=c252ae75|U=9ad7cd85|C=198f93bd|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.2|-|1|0|=|V=4896|I=-1|P=6407b2e9|N=19105777|U=2b804df9|C=a56254a7|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.3|-|1|0|=|V=2556|I=-1|P=935a806f|N=3ac14c05|U=a71addad|C=53e821de|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.4|-|1|0|=|V=720|I=-1|P=f01439fd|N=0e84bac5|U=2c029775|C=10f86cd7|A=-|X=-|M=e325459e|F=1,1,0,1
r.1.5|-|1|0|=|V=132|I=-1|P=910b28df|N=58057c47|U=8fe9af65|C=6b953eab|A=-|X=-|M=86f613cb|F=1,1,0,1
r.1.6|-|1|0|=|V=72|I=-1|P=f73b2f7d|N=a8a41ba5|U=155c3305|C=213d338d|A=-|X=-|M=e19d6c00|F=1,1,0,1
r.1.7|-|1|0|=|V=36|I=-1|P=aa34c4fd|N=f87a9e35|U=5aefde65|C=125523dd|A=-|X=-|M=871e65c5|F=1,1,0,1
r.2|quarry-haul-truck|1|9|69,-1.75,18|0,0.767798663,0,0.6406912|1,1,1
r.2.0|-|1|0|=|V=648|I=-1|P=606ebadc|N=bb406a3d|U=7d468975|C=3c557837|A=-|X=-|M=90c71f94|F=1,1,0,1
r.2.1|-|1|0|=|V=6600|I=-1|P=5887a433|N=d22e2c55|U=282ebd21|C=7438c237|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.2.2|-|1|0|=|V=1764|I=-1|P=62d92c71|N=427f3a15|U=5f60d60d|C=5a8a111b|A=-|X=-|M=e325459e|F=1,1,0,1
r.2.3|-|1|0|=|V=4068|I=-1|P=6327ac39|N=e6c6139d|U=a3fe8465|C=f8b3f409|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.2.4|-|1|0|=|V=36|I=-1|P=8e69a04d|N=f87a9e35|U=5aefde65|C=45a276ad|A=-|X=-|M=a01f6c8d|F=1,1,0,1
r.2.5|-|1|0|=|V=72|I=-1|P=e2070e0d|N=a8a41ba5|U=155c3305|C=43e3f301|A=-|X=-|M=86f613cb|F=1,1,0,1
r.2.6|-|1|0|=|V=72|I=-1|P=e1ee83d5|N=a8a41ba5|U=155c3305|C=7ecfffd5|A=-|X=-|M=de456449|F=1,1,0,1
r.2.7|-|1|0|=|V=36|I=-1|P=3939b37d|N=f87a9e35|U=5aefde65|C=292dc40b|A=-|X=-|M=e19d6c00|F=1,1,0,1
r.2.8|-|1|0|=|V=5400|I=-1|P=e34b7a00|N=44a8ade7|U=3d87bfa5|C=c92e2f99|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3|-|1|260|=
r.3.0|-|1|8|0,-1.8,-77|0,1,0,0|1,1,1
r.3.0.0|-|1|0|=|V=2880|I=-1|P=c854ca4d|N=30fd870d|U=4cdc106a|C=e52ec49e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.1|-|1|0|=|V=2880|I=-1|P=ae3699de|N=7b8d295e|U=267ab445|C=428273ac|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.2|-|1|0|=|V=2880|I=-1|P=8eb6f567|N=9b63b20f|U=7e862c41|C=212e149a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.3|-|1|0|=|V=2880|I=-1|P=809a5e35|N=aa7e5435|U=e8140cc8|C=7369b4a0|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.4|-|1|0|=|V=2880|I=-1|P=edf91096|N=c99f48b5|U=4cfa33e4|C=1c3cb6c9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.5|-|1|0|=|V=2880|I=-1|P=f4d1e1a0|N=22e5ae9d|U=e537bf9d|C=06878cf1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.6|-|1|0|=|V=2880|I=-1|P=9f0a6a56|N=9e906808|U=68a794fd|C=54f5ab4b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.7|-|1|0|=|V=2880|I=-1|P=c4b9749b|N=234db66c|U=c1dda068|C=fad0a8e7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1|-|1|8|0,2.5,-95|0,1,0,0|1,1,1
r.3.1.0|-|1|0|=|V=4032|I=-1|P=61e5f8ef|N=bb4a33dd|U=5d9caf51|C=93a47571|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.1|-|1|0|=|V=4032|I=-1|P=abbf0cda|N=e73622a8|U=75d26ef2|C=f1f4c659|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.2|-|1|0|=|V=4032|I=-1|P=6faa0ea5|N=4585010d|U=501c4ac2|C=3a6f83d4|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.3|-|1|0|=|V=4032|I=-1|P=48cf33f5|N=73311003|U=467ef276|C=0ab40a38|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.4|-|1|0|=|V=4032|I=-1|P=03a928ef|N=01c4436a|U=e0d79c8d|C=77a3df27|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.5|-|1|0|=|V=4032|I=-1|P=b93f1ede|N=6cff8ac3|U=67875690|C=72fe7e4e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.6|-|1|0|=|V=4032|I=-1|P=049f5ba6|N=2d48170f|U=f7dc6fcd|C=ddc43d5d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.7|-|1|0|=|V=4032|I=-1|P=02a2da19|N=44baaddc|U=fb9ca211|C=343db25e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2|-|1|8|0,7.8,-115|0,1,0,0|1,1,1
r.3.2.0|-|1|0|=|V=4416|I=-1|P=0ade5eb0|N=2b5045e6|U=df877992|C=a6344ccb|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.1|-|1|0|=|V=4416|I=-1|P=100c5076|N=7bf2ac34|U=8426656d|C=324b5435|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.2|-|1|0|=|V=4416|I=-1|P=f2a4ec4e|N=faf6652a|U=5c212536|C=54c4ab08|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.3|-|1|0|=|V=4416|I=-1|P=77a6d658|N=f502aa46|U=78583868|C=4af6f016|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.4|-|1|0|=|V=4416|I=-1|P=12564821|N=890db812|U=e573ed9e|C=82509517|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.5|-|1|0|=|V=4416|I=-1|P=5343223c|N=badce17d|U=09ea796a|C=990e1afe|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.6|-|1|0|=|V=4416|I=-1|P=153b2f8c|N=56d60fcd|U=663e72ef|C=c80b9959|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.7|-|1|0|=|V=4416|I=-1|P=3f772878|N=5cf82332|U=f72a3ecc|C=223b64d2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3|-|1|5|-80,-1.8,0|0,-0.707106781,0,0.707106781|1,1,1
r.3.3.0|-|1|0|=|V=1728|I=-1|P=8c62dfe2|N=85f31174|U=014e9cb1|C=537af07b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.1|-|1|0|=|V=1728|I=-1|P=ec4e4f61|N=d133ab1f|U=05b44acc|C=6e961882|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.2|-|1|0|=|V=1728|I=-1|P=0b23a6fb|N=f2335ccd|U=d4118055|C=ee825e14|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.3|-|1|0|=|V=1728|I=-1|P=9f346304|N=8fa09643|U=b16261be|C=33b7074a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.4|-|1|0|=|V=756|I=-1|P=a1b395ef|N=88d7be0c|U=b037af34|C=22026bd6|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4|-|1|5|-96,2.2,0|0,-0.707106781,0,0.707106781|1,1,1
r.3.4.0|-|1|0|=|V=3264|I=-1|P=25ba04c1|N=2e971865|U=6bbdb91c|C=01912960|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.1|-|1|0|=|V=3264|I=-1|P=a0704517|N=15fc94ec|U=b2f7a410|C=bfb7091d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.2|-|1|0|=|V=3264|I=-1|P=374e1152|N=2dd296ac|U=fbc806ef|C=9aa098e5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.3|-|1|0|=|V=3264|I=-1|P=9b3e9b68|N=79ba51f8|U=fc3be32c|C=45e76d77|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.4|-|1|0|=|V=1428|I=-1|P=9c34c3a9|N=539d8804|U=88243f88|C=89a405c1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5|-|1|8|0,-1.8,78|0,0,0,1|1,1,1
r.3.5.0|-|1|0|=|V=1728|I=-1|P=6e182d55|N=dfa8d3af|U=c780082c|C=4140865c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.1|-|1|0|=|V=1728|I=-1|P=fa81fe62|N=375a760d|U=a27a2330|C=24544e95|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.2|-|1|0|=|V=1728|I=-1|P=cf348147|N=0e9a8887|U=d9edae41|C=1b32bc33|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.3|-|1|0|=|V=1728|I=-1|P=baa0a1e4|N=9cc066e3|U=feb40f27|C=d346ce6e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.4|-|1|0|=|V=1728|I=-1|P=a8a7898f|N=beabb5b2|U=935eadf5|C=5b07d575|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.5|-|1|0|=|V=1728|I=-1|P=ebe5390f|N=1aa3c4e6|U=52fdfe60|C=399a8ed1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.6|-|1|0|=|V=1728|I=-1|P=bfe36ff9|N=3098e657|U=933932ca|C=a67c1af2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.7|-|1|0|=|V=1728|I=-1|P=bab8a0c8|N=9086f5f2|U=d98dc18e|C=891ae3cf|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6|-|1|8|0,2.5,93|0,0,0,1|1,1,1
r.3.6.0|-|1|0|=|V=3648|I=-1|P=cd95ac1f|N=c47d05a2|U=f251e502|C=5f13bd7a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.1|-|1|0|=|V=3648|I=-1|P=83bbd952|N=78c62f3b|U=95d64e4c|C=cb2a8a45|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.2|-|1|0|=|V=3648|I=-1|P=eef7fe0a|N=8126253d|U=e0b6187d|C=5d33326b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.3|-|1|0|=|V=3648|I=-1|P=785598cf|N=8f688067|U=d8dcd4fe|C=f9d06ff4|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.4|-|1|0|=|V=3648|I=-1|P=518fd4aa|N=e863f0b5|U=edd7a605|C=e789040b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.5|-|1|0|=|V=3648|I=-1|P=ab27d6c8|N=16a7852a|U=44bcf9ff|C=1986d52e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.6|-|1|0|=|V=3648|I=-1|P=37a3cc9e|N=3a7ec15b|U=b560aa65|C=3cf0ca77|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.7|-|1|0|=|V=3648|I=-1|P=26a65911|N=39f3b092|U=523abedb|C=49b3c702|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7|-|1|8|0,7.8,114|0,0,0,1|1,1,1
r.3.7.0|-|1|0|=|V=3648|I=-1|P=056a65a0|N=fcd2e82b|U=2db09a47|C=f05290ac|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.1|-|1|0|=|V=3648|I=-1|P=c6bf1b69|N=2591d1b3|U=de4e831a|C=5f2cbf4a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.2|-|1|0|=|V=3648|I=-1|P=df7ff964|N=4a83f7f5|U=e4ea5512|C=b60c38a8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.3|-|1|0|=|V=3648|I=-1|P=a6290660|N=f89a30e3|U=2191afcc|C=4b604ad8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.4|-|1|0|=|V=3648|I=-1|P=c181ba3a|N=ff536e68|U=4c2c4d89|C=e14209f2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.5|-|1|0|=|V=3648|I=-1|P=292a2283|N=80b46ee6|U=7b2b2da5|C=806684b5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.6|-|1|0|=|V=3648|I=-1|P=c7b9c5f0|N=c405b895|U=28d894fc|C=dfe5f32d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.7|-|1|0|=|V=3648|I=-1|P=010fb8eb|N=ced8154e|U=bec32ead|C=e445878a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8|-|1|5|78,-1.8,0|0,0.707106781,0,0.707106781|1,1,1
r.3.8.0|-|1|0|=|V=2880|I=-1|P=8fb76a14|N=59a65cd5|U=b2459421|C=5414e7e4|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.1|-|1|0|=|V=2880|I=-1|P=aece0edf|N=f8ac5095|U=82c8a754|C=cb26b83b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.2|-|1|0|=|V=2880|I=-1|P=233ac0a1|N=41ee2e2f|U=96b5b820|C=58b139e6|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.3|-|1|0|=|V=2880|I=-1|P=446f24c7|N=3dce61cc|U=38f86e94|C=63f1fbc7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.4|-|1|0|=|V=1260|I=-1|P=bed4a3c4|N=d602264b|U=fc36170e|C=31efa8da|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9|-|1|5|97,2.2,0|0,0.707106781,0,0.707106781|1,1,1
r.3.9.0|-|1|0|=|V=2880|I=-1|P=80d8e9ad|N=af9adaf1|U=ba099702|C=4f21a691|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.1|-|1|0|=|V=2880|I=-1|P=e2190eab|N=28ec4e3b|U=48ec2e4e|C=739c2c04|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.2|-|1|0|=|V=2880|I=-1|P=3ca04787|N=ff8d15da|U=73555244|C=95f23cd3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.3|-|1|0|=|V=2880|I=-1|P=d85bf276|N=46e79fbe|U=510c8e93|C=f68ea26f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.4|-|1|0|=|V=1260|I=-1|P=cc45607c|N=6bd88d9f|U=160ae6c0|C=f03172c1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.10|-|1|0|=|V=1029|I=5256|P=6770e728|N=e4c986ee|U=da31e729|C=7e87937b|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.11|-|1|0|=|V=1029|I=5256|P=649bc15c|N=16a271ba|U=187b71ca|C=e46ae406|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.12|-|1|0|=|V=994|I=5076|P=1b9969ed|N=e0205c00|U=e3951690|C=b719b383|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.13|-|1|0|=|V=637|I=3240|P=12a2176d|N=61970bd3|U=af86b8c8|C=767156a5|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.14|-|1|0|=|V=325|I=1836|P=9412e326|N=688befb2|U=67c7a736|C=a4b3eeb7|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.15|-|1|0|=|V=121320|I=-1|P=d9225269|N=6285f97b|U=314277e5|C=2e218866|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.16|-|1|0|53.4418097,-1.47769216,65.3015381|0,-0.248457977,0,0.968642676|1,1,1|V=336|I=-1|P=f84768cb|N=4e7a934c|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.17|-|1|0|11.5612569,-0.954262195,-64.1383604|0,0.117717649,0,0.993047106|1,1,1|V=720|I=-1|P=b98a53d1|N=600fc715|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.18|-|1|0|-2.69994512,-1.792,66.8020833|0,0.253605816,0,0.96730765|1,1,1|V=720|I=-1|P=a61c97e1|N=31d985b0|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.19|-|1|0|-48.9208838,-1.792,-67.0518412|0,0.0595308913,0,0.998226464|1,1,1|V=1104|I=-1|P=b5bcda56|N=5ee74d51|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.20|-|1|0|28.1156319,-1.29775483,64.9016774|0,-0.277700753,0,0.960667628|1,1,1|V=336|I=-1|P=7532b01c|N=946dc1f9|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.21|-|1|0|-46.6000823,-1.14344502,-64.5587667|0,-0.313796598,0,0.949490229|1,1,1|V=720|I=-1|P=a0b45760|N=84a21914|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.22|-|1|0|44.4080773,-1.792,67.9264632|0,-0.445843696,0,0.89511083|1,1,1|V=720|I=-1|P=8f177b13|N=9cf6c81f|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.23|-|1|0|-29.3710009,-1.49848239,-65.3477386|0,-0.31791694,0,0.948118568|1,1,1|V=1104|I=-1|P=565f42ef|N=a45eb88b|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.24|-|1|0|-23.7655446,-1.792,66.7772296|0,-0.451022152,0,0.892512755|1,1,1|V=336|I=-1|P=f3839316|N=7ce49b30|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.25|-|1|0|-60.245454,-1.792,-66.0064346|0,-0.301808867,0,0.953368453|1,1,1|V=720|I=-1|P=ed7ede9f|N=0150abed|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.26|-|1|0|-3.67290316,-1.792,69.8801957|0,-0.431522192,0,0.902102321|1,1,1|V=720|I=-1|P=2e630be7|N=2bb30ef3|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.27|-|1|0|25.0121961,-1.792,-67.5945274|0,-0.16328635,0,0.986578719|1,1,1|V=1104|I=-1|P=000515e0|N=7ac75f07|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.28|-|1|0|53.2066484,-1.792,67.0761624|0,0.0260808147,0,0.999659838|1,1,1|V=336|I=-1|P=f84768cb|N=4e7a934c|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.29|-|1|0|-36.0437085,-1.792,-66.3119011|0,-0.244566075,0,0.969632629|1,1,1|V=720|I=-1|P=b98a53d1|N=600fc715|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.30|-|1|0|-19.4125404,-1.36848599,65.0588578|0,0.419225892,0,0.907881959|1,1,1|V=720|I=-1|P=a61c97e1|N=31d985b0|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.31|-|1|0|-43.7779759,-1.59067533,-65.5526119|0,0.313892929,0,0.949458387|1,1,1|V=1104|I=-1|P=b5bcda56|N=5ee74d51|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.32|-|1|0|12.118863,-1.792,67.545617|0,-0.252229166,0,0.96766753|1,1,1|V=336|I=-1|P=7532b01c|N=946dc1f9|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.33|-|1|0|-60.8312014,-1.04870996,-64.3482444|0,0.384651972,0,0.923061677|1,1,1|V=720|I=-1|P=a0b45760|N=84a21914|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.34|-|1|0|56.8982346,-0.965907968,64.1642399|0,-0.410349976,0,0.911928121|1,1,1|V=720|I=-1|P=8f177b13|N=9cf6c81f|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.35|-|1|0|15.6800286,-1.60891152,-65.5931367|0,-0.134866838,0,0.990863732|1,1,1|V=1104|I=-1|P=565f42ef|N=a45eb88b|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.36|-|1|0|-23.4142764,-1.792,67.9819035|0,-0.335279092,0,0.942118851|1,1,1|V=336|I=-1|P=f3839316|N=7ce49b30|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.37|-|1|0|24.9816007,-1.792,-68.2192064|0,-0.420813299,0,0.907147269|1,1,1|V=720|I=-1|P=ed7ede9f|N=0150abed|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.38|-|1|0|9.47656552,-1.792,68.8020492|0,-0.258646743,0,0.965971978|1,1,1|V=720|I=-1|P=2e630be7|N=2bb30ef3|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.39|-|1|0|32.099017,-1.792,-66.6731991|0,0.152393049,0,0.988319968|1,1,1|V=1104|I=-1|P=000515e0|N=7ac75f07|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.40|-|1|0|-1.87008293,-1.792,69.9976535|0,-0.208030137,0,0.978122417|1,1,1|V=336|I=-1|P=f84768cb|N=4e7a934c|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.41|-|1|0|-55.783674,-1.792,-68.9809425|0,-0.206180006,0,0.97851408|1,1,1|V=720|I=-1|P=b98a53d1|N=600fc715|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.42|-|1|0|-51.1684044,-1.792,67.5878489|0,0.291019412,0,0.956717148|1,1,1|V=720|I=-1|P=a61c97e1|N=31d985b0|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.43|-|1|0|-31.1858579,-1.792,-69.7955642|0,0.399078982,0,0.916916553|1,1,1|V=1104|I=-1|P=b5bcda56|N=5ee74d51|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.44|-|1|0|-47.193834,-1.792,67.667591|0,0.398820626,0,0.917028957|1,1,1|V=336|I=-1|P=7532b01c|N=946dc1f9|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.45|-|1|0|16.0283491,-1.792,-69.1583298|0,-0.478228128,0,0.87823565|1,1,1|V=720|I=-1|P=a0b45760|N=84a21914|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.46|-|1|0|38.966499,-1.15245461,64.578788|0,-0.138374204,0,0.990380018|1,1,1|V=720|I=-1|P=8f177b13|N=9cf6c81f|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.47|-|1|0|34.4395304,-1.792,-69.3199753|0,-0.302129061,0,0.95326703|1,1,1|V=1104|I=-1|P=565f42ef|N=a45eb88b|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.48|-|1|0|39.6885582,-1.792,66.7125111|0,0.109516229,0,0.993985008|1,1,1|V=336|I=-1|P=f3839316|N=7ce49b30|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.49|-|1|0|21.5423346,-1.39433403,-65.1162978|0,0.388156381,0,0.921593524|1,1,1|V=720|I=-1|P=ed7ede9f|N=0150abed|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.50|-|1|0|-14.5380362,-1.03769861,64.3237747|0,-0.289171289,0,0.957277371|1,1,1|V=720|I=-1|P=2e630be7|N=2bb30ef3|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.51|-|1|0|-9.77980259,-1.792,-66.4151254|0,0.100185938,0,0.994968732|1,1,1|V=1104|I=-1|P=000515e0|N=7ac75f07|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.52|-|1|0|37.5160893,-1.792,66.4286164|0,-0.166270097,0,0.986080248|1,1,1|V=336|I=-1|P=f84768cb|N=4e7a934c|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.53|-|1|0|27.0629233,-1.08340287,-64.4253397|0,-0.423598494,0,0.905850051|1,1,1|V=720|I=-1|P=b98a53d1|N=600fc715|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.54|-|1|0|-25.1396514,-1.792,69.1605661|0,0.325424413,0,0.945568058|1,1,1|V=720|I=-1|P=a61c97e1|N=31d985b0|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.55|-|1|0|36.2431744,-1.792,-68.4139779|0,-0.205693342,0,0.978616497|1,1,1|V=1104|I=-1|P=b5bcda56|N=5ee74d51|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.56|-|1|0|47.0139145,-1.792,67.0520354|0,-0.478575246,0,0.878046545|1,1,1|V=336|I=-1|P=7532b01c|N=946dc1f9|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.57|-|1|0|59.6812976,-1.792,-69.4436031|0,0.363946633,0,0.931419802|1,1,1|V=720|I=-1|P=a0b45760|N=84a21914|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.58|-|1|0|28.181935,-1.792,66.7238874|0,-0.0684532945,0,0.997654322|1,1,1|V=720|I=-1|P=8f177b13|N=9cf6c81f|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.59|-|1|0|-26.8854947,-1.71038387,-65.8186308|0,0.42788652,0,0.903832466|1,1,1|V=1104|I=-1|P=565f42ef|N=a45eb88b|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.60|-|1|0|52.80816,-1.60978173,65.5950705|0,0.311429567,0,0.950269238|1,1,1|V=336|I=-1|P=f3839316|N=7ce49b30|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.61|-|1|0|23.4966986,-1.792,-69.5463245|0,-0.218788979,0,0.975772198|1,1,1|V=720|I=-1|P=ed7ede9f|N=0150abed|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.62|-|1|0|43.8714052,-1.792,68.7261556|0,0.221767884,0,0.975099485|1,1,1|V=720|I=-1|P=2e630be7|N=2bb30ef3|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.63|-|1|0|-56.9883431,-1.792,-67.0636596|0,0.164106242,0,0.98644267|1,1,1|V=1104|I=-1|P=000515e0|N=7ac75f07|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.64|-|1|0|48.2766497,-1.25947888,64.8166197|0,0.337685499,0,0.941258999|1,1,1|V=336|I=-1|P=f84768cb|N=4e7a934c|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.65|-|1|0|28.7988155,-1.57759659,-65.523548|0,-0.311184258,0,0.950349598|1,1,1|V=720|I=-1|P=b98a53d1|N=600fc715|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.66|-|1|0|-42.952932,-1.792,67.5391956|0,-0.332212162,0,0.943204686|1,1,1|V=720|I=-1|P=a61c97e1|N=31d985b0|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.67|-|1|0|-5.61496164,-1.4705256,-65.2856124|0,-0.026632362,0,0.999645296|1,1,1|V=1104|I=-1|P=b5bcda56|N=5ee74d51|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.68|-|1|0|6.27208063,-1.792,67.0991863|0,0.293234551,0,0.956040532|1,1,1|V=336|I=-1|P=7532b01c|N=946dc1f9|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.69|-|1|0|-16.9546634,-1.65114034,-65.6869785|0,-0.0201837295,0,0.999796288|1,1,1|V=720|I=-1|P=a0b45760|N=84a21914|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.70|-|1|0|-30.9652712,-1.792,68.4897862|0,0.405907024,0,0.913914377|1,1,1|V=720|I=-1|P=8f177b13|N=9cf6c81f|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.71|-|1|0|38.114139,-1.08449402,-64.4277645|0,0.345765665,0,0.938320896|1,1,1|V=1104|I=-1|P=565f42ef|N=a45eb88b|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.72|-|1|0|-47.4741757,-0.985639075,64.2080868|0,-0.371217131,0,0.92854609|1,1,1|V=336|I=-1|P=f3839316|N=7ce49b30|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.73|-|1|0|33.9032976,-1.792,-67.3746745|0,0.247676415,0,0.968842812|1,1,1|V=720|I=-1|P=ed7ede9f|N=0150abed|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.74|-|1|0|51.8243929,-1.36051469,65.0411438|0,-0.17398332,0,0.9847486|1,1,1|V=720|I=-1|P=2e630be7|N=2bb30ef3|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.75|-|1|0|30.9480081,-1.792,-66.1049447|0,0.35701772,0,0.934097611|1,1,1|V=1104|I=-1|P=000515e0|N=7ac75f07|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.76|-|1|0|-44.1839964,-1.76185659,65.9330146|0,-0.29407469,0,0.955782442|1,1,1|V=336|I=-1|P=f84768cb|N=4e7a934c|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.77|-|1|0|31.8512436,-1.792,-68.2959726|0,-0.243262586,0,0.96996047|1,1,1|V=720|I=-1|P=b98a53d1|N=600fc715|U=5cba87fe|C=2ee1fa6c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.78|-|1|0|-0.861673445,-1.1499712,64.5732693|0,0.108953969,0,0.994046796|1,1,1|V=720|I=-1|P=a61c97e1|N=31d985b0|U=c9ddfce5|C=f81893e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.79|-|1|0|-8.5929087,-1.58954958,-65.5501102|0,-0.230537026,0,0.973063554|1,1,1|V=1104|I=-1|P=b5bcda56|N=5ee74d51|U=b91cb47d|C=91c271a1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.80|-|1|0|52.8423638,-1.792,67.0901777|0,0.476828553,0,0.87899632|1,1,1|V=336|I=-1|P=7532b01c|N=946dc1f9|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.81|-|1|0|67.5435062,-1.932,-29.8622538|0,-0.441795081,0,0.897115994|1,1,1|V=1104|I=-1|P=d2a7ce95|N=7f9ac9fb|U=b1f348d7|C=89a225a9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.82|-|1|0|65.3733601,-1.65001206,6.68824688|0,-0.594856883,0,0.80383163|1,1,1|V=1488|I=-1|P=6e964722|N=e232d967|U=12eeaf47|C=0763b367|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.83|-|1|0|-67.6598833,-1.932,47.7880139|0,-0.992077419,0,0.125628|1,1,1|V=1488|I=-1|P=60951388|N=cbff9070|U=c3c9636d|C=6ff4b351|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.84|-|1|0|66.3236551,-1.932,-0.400768523|0,-0.492058546,0,0.87056211|1,1,1|V=1104|I=-1|P=e8906d13|N=b12c787f|U=c1e7743d|C=44b96131|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.85|-|1|0|-66.1254748,-1.932,29.3920618|0,-0.998801782,0,0.0489387437|1,1,1|V=1104|I=-1|P=50d541cc|N=a7a4a532|U=b1f348d7|C=39ab1499|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.86|-|1|0|69.3328496,-1.932,6.94303822|0,-0.964664401,0,0.263481675|1,1,1|V=1104|I=-1|P=7de4c895|N=b281258c|U=6decd08b|C=cfd48eea|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.87|-|1|0|-64.5053863,-1.25942384,-29.6631832|0,0.904703767,0,0.426041189|1,1,1|V=720|I=-1|P=241121e1|N=cd95997d|U=5e541a28|C=a9bdc0c7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.88|-|1|0|-66.5670903,-1.932,-26.6574722|0,0.753219041,0,0.657769775|1,1,1|V=1104|I=-1|P=cf5c51b9|N=600aa635|U=6decd08b|C=d453cf4d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.89|-|1|0|-70.4058365,-1.932,-19.9776851|0,-0.738824017,0,0.673898414|1,1,1|V=1488|I=-1|P=d549fafd|N=3625aac0|U=c3c9636d|C=3f9e3ad5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.90|-|1|0|-68.7370428,-1.932,-8.83575224|0,0.234092502,0,0.972214328|1,1,1|V=1488|I=-1|P=faeaa817|N=67c62093|U=28d0269e|C=32e1da1d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.91|-|1|0|67.6551596,-1.932,3.82818438|0,-0.180086461,0,0.983650785|1,1,1|V=1488|I=-1|P=73656e4a|N=3e2307d9|U=92839259|C=b0a1764c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.92|-|1|0|-69.1907948,-1.932,-34.9546219|0,-0.104002023,0,0.994577086|1,1,1|V=1104|I=-1|P=b5b67c0d|N=ca1c535b|U=5366bf15|C=f3a2bf8a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.93|-|1|0|67.7026624,-1.932,7.84011993|0,0.943926446,0,0.330155818|1,1,1|V=1488|I=-1|P=7348e1b0|N=ed487eec|U=c3c9636d|C=4a89556c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.94|-|1|0|-67.1779262,-1.932,43.4654455|0,-0.570256134,0,0.821466945|1,1,1|V=1488|I=-1|P=5ec324fa|N=e9afe596|U=6756e2a0|C=111fffd8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.95|-|1|0|-65.2395308,-1.58978885,19.2381791|0,-0.999997099,0,0.00240888978|1,1,1|V=720|I=-1|P=fae56258|N=79912124|U=5cba87fe|C=989abc17|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.96|-|1|0|67.7116866,-1.932,-26.6758121|0,-0.99960466,0,0.0281162496|1,1,1|V=1104|I=-1|P=5b0c5fc1|N=3d975b8a|U=5366bf15|C=0920d454|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.97|-|1|0|-65.5102961,-1.71163323,-19.6858755|0,-0.0487040566,0,0.998813253|1,1,1|V=1488|I=-1|P=4bde1111|N=440693a5|U=c3c9636d|C=6820a41d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.98|-|1|0|66.8902038,-1.932,-37.0360571|0,-0.531215986,0,0.847236434|1,1,1|V=1104|I=-1|P=7a1f3324|N=bf551c18|U=c1e7743d|C=5c3b4612|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.99|-|1|0|-65.776657,-1.83149565,18.9018475|0,0.766520585,0,0.642219739|1,1,1|V=1104|I=-1|P=7860257e|N=56523eaf|U=b1f348d7|C=64fba0f9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.100|-|1|0|-69.4795117,-1.932,38.7451467|0,-0.817462617,0,0.575981658|1,1,1|V=1104|I=-1|P=285a9a7e|N=1c6ffc07|U=5366bf15|C=58bef029|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.101|-|1|0|-69.5086354,-1.932,-41.3552374|0,-0.930800099,0,0.365528626|1,1,1|V=720|I=-1|P=c8e2c1fe|N=9cc78154|U=5e541a28|C=8564a8c1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.102|-|1|0|-70.3238675,-1.932,-26.9562622|0,0.99325525,0,0.115948301|1,1,1|V=1104|I=-1|P=c05979fc|N=3614d71f|U=6decd08b|C=be6243e4|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.103|-|1|0|=|V=175|I=816|P=75e74885|N=59e18c86|U=5a9056c7|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.104|-|1|0|=|V=175|I=816|P=4a61534a|N=d2025466|U=fc1ec8ba|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.105|-|1|0|=|V=175|I=816|P=8c36bd48|N=1fccc281|U=95189c4d|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.106|-|1|0|=|V=175|I=816|P=89b83b89|N=e6fb9976|U=224182e1|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.107|-|1|0|=|V=30|I=120|P=3952edd7|N=4fec64be|U=a77620d0|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.108|-|1|0|=|V=175|I=816|P=073af0b7|N=28d60610|U=c8ea3b07|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.109|-|1|0|=|V=175|I=816|P=e47bbbc2|N=b1181eab|U=bef52566|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.110|-|1|0|=|V=175|I=816|P=fb6bae8a|N=0db1db39|U=ea51e56d|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.111|-|1|0|=|V=175|I=816|P=fb9b73e8|N=531a72e0|U=8e967809|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.112|-|1|0|=|V=30|I=120|P=135c1616|N=3d110b93|U=758d549c|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.113|-|1|0|=|V=175|I=816|P=11ce6e29|N=3981d08e|U=1358857b|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.114|-|1|0|=|V=175|I=816|P=801eae72|N=b013be26|U=dca7ab9e|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.115|-|1|0|=|V=175|I=816|P=a6e06a68|N=6c626df1|U=2d233ca0|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.116|-|1|0|=|V=175|I=816|P=c05b473d|N=895f4326|U=c586bceb|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.117|-|1|0|=|V=30|I=120|P=cd38ea03|N=b925c386|U=99737e1a|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.118|-|1|0|=|V=175|I=816|P=17a39a73|N=7586aac8|U=ce15593c|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.119|-|1|0|=|V=175|I=816|P=67c661aa|N=e51e3357|U=ea0c2b61|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.120|-|1|0|=|V=175|I=816|P=feac9cd6|N=23d0da95|U=7281757b|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.121|-|1|0|=|V=175|I=816|P=056e42cc|N=7980ea14|U=0f1c5190|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.122|-|1|0|=|V=30|I=120|P=87e9e186|N=0eed8aab|U=932404de|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.123|-|1|0|=|V=175|I=816|P=d9985ead|N=dd730657|U=b14d9151|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.124|-|1|0|=|V=175|I=816|P=c3f0527c|N=897b7e20|U=495e5f54|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.125|-|1|0|=|V=175|I=816|P=9ceb8fbd|N=a338dc4f|U=dc7f71f7|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.126|-|1|0|=|V=175|I=816|P=31f3c3bd|N=aa022c05|U=476472ab|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.127|-|1|0|=|V=30|I=120|P=c3914706|N=3a15facb|U=ed265ea8|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.128|-|1|0|=|V=175|I=816|P=d71a99ba|N=f40d6066|U=9b06cd38|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.129|-|1|0|=|V=175|I=816|P=11fa804d|N=045aeed3|U=dec01481|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.130|-|1|0|=|V=175|I=816|P=9f29c641|N=14c827c7|U=7c8e16f2|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.131|-|1|0|=|V=175|I=816|P=1dfbe26b|N=49dcdc92|U=2f7c4c7e|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.132|-|1|0|=|V=30|I=120|P=5620570e|N=39dd8bad|U=cc571cfc|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.133|-|1|0|=|V=175|I=816|P=d5098f95|N=a8bd6967|U=7339bc09|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.134|-|1|0|=|V=175|I=816|P=c0cb7628|N=eb1ec908|U=966f095c|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.135|-|1|0|=|V=175|I=816|P=dbbddaf1|N=10ce7d57|U=fc9d9e26|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.136|-|1|0|=|V=175|I=816|P=61a56d8d|N=c02cbd21|U=3ee9bf11|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.137|-|1|0|=|V=30|I=120|P=fda43026|N=5af2b7e3|U=132df832|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.138|-|1|0|=|V=175|I=816|P=43222ad2|N=202a02e2|U=20f2e941|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.139|-|1|0|=|V=175|I=816|P=a21f51f9|N=94da6afb|U=45d47eb8|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.140|-|1|0|=|V=175|I=816|P=c531e979|N=90fbd577|U=299a1c8a|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.141|-|1|0|=|V=175|I=816|P=b20b9f37|N=a309506e|U=bd6a75d9|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.142|-|1|0|=|V=30|I=120|P=e040bcbe|N=b8f2e66d|U=955facae|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.143|-|1|0|=|V=255|I=1200|P=a7542d89|N=f7156afa|U=3266c08e|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.144|-|1|0|=|V=255|I=1200|P=a210c6fc|N=042c5277|U=9a2e93e7|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.145|-|1|0|=|V=255|I=1200|P=0bf42de0|N=3e5acadd|U=7d91e18e|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.146|-|1|0|=|V=255|I=1200|P=c28041be|N=6aec374a|U=70d9285b|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.147|-|1|0|=|V=40|I=168|P=d66d2b5b|N=7f4e21ea|U=2a53a6d3|C=8b67d60d|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.148|-|1|0|=|V=255|I=1200|P=e0eb6eda|N=c0d477e9|U=7345f7f3|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.149|-|1|0|=|V=255|I=1200|P=0cd7f1a0|N=c73469e7|U=a2bf6024|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.150|-|1|0|=|V=255|I=1200|P=0adb5162|N=fed1bf39|U=935bccb2|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.151|-|1|0|=|V=255|I=1200|P=e49f1c1b|N=64ab3705|U=69b9a1c6|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.152|-|1|0|=|V=40|I=168|P=be0afa9f|N=5239cea2|U=b80b529a|C=8b67d60d|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.153|-|1|0|=|V=255|I=1200|P=46dc847b|N=da4582c3|U=f27efa23|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.154|-|1|0|=|V=255|I=1200|P=dd7426cc|N=2a7f948b|U=470b56d3|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.155|-|1|0|=|V=255|I=1200|P=e1b970ed|N=1ba1ce04|U=17f162e2|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.156|-|1|0|=|V=255|I=1200|P=92e48f74|N=f18c1621|U=5a76011a|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.157|-|1|0|=|V=40|I=168|P=13b98548|N=d26b7764|U=f40e0a9f|C=8b67d60d|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.158|-|1|0|=|V=255|I=1200|P=7c0d217e|N=e5ac1d98|U=6b6d53cb|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.159|-|1|0|=|V=255|I=1200|P=111dbafd|N=4e154a77|U=60d2c2f3|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.160|-|1|0|=|V=255|I=1200|P=e8be5491|N=5338098f|U=d70150f9|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.161|-|1|0|=|V=255|I=1200|P=df140e8e|N=4054a59b|U=c17a9193|C=3e7499a8|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.162|-|1|0|=|V=40|I=168|P=41fb2a12|N=f42d9301|U=b87edbe4|C=8b67d60d|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.163|-|1|0|=|V=2016|I=7968|P=f07253ed|N=f87e2a45|U=552496b5|C=a58701e5|A=b419f4c5|X=-|M=7f8ad734|F=0,1,0,1
r.3.164|-|1|0|=|V=533|I=3186|P=34b79559|N=52027523|U=c05d883b|C=6911e147|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.165|-|1|0|=|V=20640|I=-1|P=17ba82f9|N=1e7f61f4|U=cdf4f0c5|C=fee37aed|A=-|X=-|M=25f445b9|F=0,0,0,1
r.3.166|-|1|0|=|V=413|I=2466|P=48fac337|N=4694c2df|U=150525c0|C=1bb480e7|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.167|-|1|0|=|V=15840|I=-1|P=d67e8449|N=77bea8f0|U=5da406c5|C=35cdb0b1|A=-|X=-|M=25f445b9|F=0,0,0,1
r.3.168|-|1|0|=|V=488|I=2916|P=882b4941|N=36226728|U=6f6247a4|C=55f82925|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.169|-|1|0|=|V=17460|I=-1|P=ef29f7c1|N=d46256a5|U=df878425|C=7d62ce91|A=-|X=-|M=25f445b9|F=0,0,0,1
r.3.170|-|1|0|=|V=383|I=2286|P=0ef4bae6|N=1e662f13|U=e03b90ab|C=cc7e372f|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.171|-|1|0|=|V=13080|I=-1|P=9f50a006|N=a9da254a|U=c66fb905|C=f7242921|A=-|X=-|M=25f445b9|F=0,0,0,1
r.3.172|-|1|0|=|V=458|I=2736|P=289f0299|N=cb7792ed|U=0381c086|C=3637f07d|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.173|-|1|0|=|V=20340|I=-1|P=8cbce311|N=4ef447aa|U=0728aa25|C=ad863d51|A=-|X=-|M=25f445b9|F=0,0,0,1
r.3.174|-|1|0|=|V=413|I=2466|P=fb31876d|N=02b147f2|U=5f7f4250|C=1bb480e7|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.175|-|1|0|=|V=15840|I=-1|P=141e14e5|N=6c6face3|U=5da406c5|C=5614463d|A=-|X=-|M=25f445b9|F=0,0,0,1
r.3.176|-|1|0|=|V=2976|I=17202|P=9bb95512|N=facad331|U=8556774d|C=6af784d4|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.3.177|-|1|0|72.3167002,2.22919328,34.5|0,0.739082476,0,0.673614945|1,1,1|V=336|I=-1|P=7219b100|N=b7b84604|U=6716e645|C=3ede81bb|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.178|-|1|0|72.4061484,2.17606008,38.8065454|0,0.463050536,0,0.88633188|1,1,1|V=336|I=-1|P=edd6a75d|N=c2196f21|U=f656331a|C=cac79816|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.179|-|1|0|72.1896816,2.02398207,42.2155039|0,0.0927448153,0,0.995689911|1,1,1|V=720|I=-1|P=c0b81415|N=8b022648|U=c9ddfce5|C=3542e952|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.180|-|1|0|72.0933678,1.5923361,45.4193106|0,0.833882494,0,0.551942013|1,1,1|V=336|I=-1|P=4a2ed188|N=2b786931|U=ff16e645|C=5db2e07f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.181|-|1|0|72.3520699,0.665658129,50.4926086|0,-0.845551883,0,0.53389326|1,1,1|V=336|I=-1|P=7cf27775|N=f1712a59|U=36d6e645|C=05e72fc7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.182|-|1|0|72.3205345,-0.106701004,54.496324|0,-0.898517242,0,0.438938226|1,1,1|V=336|I=-1|P=43494583|N=123a4419|U=6716e645|C=bc52ecef|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.183|-|1|0|72.3069881,-0.793234174,58.3899276|0,-0.884405889,0,0.466718571|1,1,1|V=720|I=-1|P=592bf013|N=b6d80010|U=5cba87fe|C=65bdbe0d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.184|-|1|0|74.7,2.25,32.4|0,0.33808304,0,0.941116283|1,1,1|V=336|I=-1|P=4dbef1bb|N=05b383de|U=193e046a|C=a734addc|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.185|-|1|0|77.8700953,2.25,32.4|0,0.0118779539,0,0.999929455|1,1,1|V=336|I=-1|P=762f691f|N=fa4b3083|U=ff16e645|C=53603e02|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.186|-|1|0|69.4643904,0.084934499,37.2180165|0,-0.736423567,0,0.676520753|1,1,1|V=336|I=-1|P=c3c9c08c|N=85a26e5a|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.187|-|1|0|66.7673939,-2.04842069,31.2602487|0,-0.839588939,0,0.543222251|1,1,1|V=336|I=-1|P=c7597b6c|N=63f6f8e7|U=f656331a|C=56a74d82|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.188|-|1|0|66.7275625,-1.84057518,31.9727159|0,-0.162721082,0,0.986672108|1,1,1|V=336|I=-1|P=6cdf91fa|N=79f379a1|U=193e046a|C=129a30be|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.189|-|1|0|67.9312601,-1.14512924,38.8056163|0,-0.968680568,0,0.248310202|1,1,1|V=336|I=-1|P=1c3e2665|N=4b1a3f11|U=ff16e645|C=7f2250ab|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.190|-|1|0|70.4814686,-1.96823228,57.1034321|0,0.674527106,0,0.738250081|1,1,1|V=336|I=-1|P=67c4be40|N=57e8b7a3|U=36d6e645|C=f7937618|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.191|-|1|0|69.6268541,-1.26492258,50.3950674|0,-0.479448407,0,0.877570068|1,1,1|V=336|I=-1|P=9e6acf34|N=b6e988f0|U=63f6e645|C=989f931b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.192|-|1|0|70.8249539,-1.43358613,56.4310365|0,-0.406715356,0,0.913554935|1,1,1|V=336|I=-1|P=425beb4b|N=4a16eee0|U=80b6e645|C=f6af9aa1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.193|-|1|0|69.7774414,-2.122,55.3821003|0,-0.955616802,0,0.294612506|1,1,1|V=336|I=-1|P=10ca42ea|N=a1cfae97|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.194|-|1|0|67.5440435,-2.122,48.2630051|0,0.257179973,0,0.966363525|1,1,1|V=336|I=-1|P=3b2814db|N=070bd617|U=f656331a|C=da8241ad|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.195|-|1|0|66.0593453,-2.11874848,32.5707024|0,-0.179846794,0,0.983694633|1,1,1|V=336|I=-1|P=25f31c44|N=aa6a247c|U=193e046a|C=96ff952f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.196|-|1|0|70.2037523,0.631200548,33.9164332|0,-0.986323322,0,0.164822038|1,1,1|V=336|I=-1|P=95330de1|N=6a82f233|U=ff16e645|C=91d42860|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.197|-|1|0|67.0748065,-1.93359481,30.5559611|0,-0.834847937,0,0.550480629|1,1,1|V=336|I=-1|P=a44e751c|N=fffa5394|U=36d6e645|C=8b8e251c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.198|-|1|0|68.2260257,-1.44725878,45.1714182|0,-0.382206201,0,0.924077064|1,1,1|V=336|I=-1|P=da350af5|N=e9e8365c|U=63f6e645|C=fb87c8c2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.199|-|1|0|67.2211069,-2.122,44.6933531|0,0.985917318,0,0.167233495|1,1,1|V=336|I=-1|P=82fc77c0|N=85c30d92|U=80b6e645|C=14f56b85|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.200|-|1|0|67.5277409,-1.506653,34.0303124|0,0.911453604,0,0.411402877|1,1,1|V=336|I=-1|P=ebad41f9|N=00ba7d29|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.201|-|1|0|68.9446975,-2.122,53.4787933|0,0.121106855,0,0.992639476|1,1,1|V=336|I=-1|P=8093a552|N=4adf9471|U=f656331a|C=49be122f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.202|-|1|0|70.6442567,-0.071306637,48.3125732|0,-0.998640853,0,0.0521195432|1,1,1|V=336|I=-1|P=cf177a83|N=4be99fad|U=193e046a|C=b32f0e75|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.203|-|1|0|67.8423577,-1.69224855,43.9990274|0,0.972726092,0,0.231956786|1,1,1|V=336|I=-1|P=16a2cb5b|N=289dcf35|U=ff16e645|C=8e0f6e9f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.204|-|1|0|69.7801485,0.320019213,39.4563932|0,0.419870007,0,0.907584254|1,1,1|V=336|I=-1|P=7b80d171|N=ad69ff3c|U=36d6e645|C=b9c22b37|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.205|-|1|0|70.0017267,-2.122,56.57123|0,-0.0369839737,0,0.999315859|1,1,1|V=336|I=-1|P=bdb2d315|N=802b8a70|U=63f6e645|C=5c85904a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.206|-|1|0|68.3417938,-1.77614896,47.4923551|0,-0.456229894,0,0.889861947|1,1,1|V=336|I=-1|P=7187e7b0|N=f973a31f|U=80b6e645|C=45a9f246|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.207|-|1|0|69.6572918,-2.122,55.7201387|0,-0.960975372,0,0.276633937|1,1,1|V=336|I=-1|P=b03a5109|N=2adc50ee|U=6716e645|C=0123d3b2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.208|-|1|0|66.2365862,-2.122,37.4705971|0,-0.382727846,0,0.923861134|1,1,1|V=336|I=-1|P=2a8c23e6|N=55cf9bbb|U=f656331a|C=f1970454|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.209|-|1|0|70.1489556,0.641801891,38.2881675|0,0.424455718,0,0.905448697|1,1,1|V=336|I=-1|P=ba96f7a1|N=8a44e7aa|U=193e046a|C=08b4e473|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.210|-|1|0|67.9446194,-2.122,48.5359207|0,0.559490681,0,0.828836641|1,1,1|V=336|I=-1|P=9e914fc4|N=24a487e9|U=ff16e645|C=307a0fa8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.211|-|1|0|68.6747351,-0.561494909,36.4670498|0,0.126810245,0,0.991926994|1,1,1|V=336|I=-1|P=30ecbc42|N=cf28f4bb|U=36d6e645|C=a36b1f6e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.212|-|1|0|68.395982,-0.808162257,35.7913974|0,0.683881922,0,0.729592706|1,1,1|V=336|I=-1|P=d380f03e|N=020bd8ab|U=63f6e645|C=e97d309d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.213|-|1|0|66.548475,-2.122,30.285039|0,0.373084726,0,0.927797277|1,1,1|V=336|I=-1|P=6ceb2d1a|N=613295e2|U=80b6e645|C=3e65f95a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.214|-|1|0|67.6268111,-1.37934005,39.624906|0,-0.976896154,0,0.213714541|1,1,1|V=336|I=-1|P=728d38fc|N=f37e304a|U=6716e645|C=152dcd5b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.215|-|1|0|69.7880871,-2.122,56.1046424|0,0.502759687,0,0.864426224|1,1,1|V=336|I=-1|P=126f9e4a|N=57e14d8e|U=f656331a|C=5518c791|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.216|-|1|0|70.6749371,0.0276607438,47.7479524|0,0.974802062,0,0.223071602|1,1,1|V=336|I=-1|P=f5f3f038|N=f4933080|U=193e046a|C=40c07f32|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.217|-|1|0|69.3721929,-0.0163965245,40.015036|0,-0.701464679,0,0.712704219|1,1,1|V=336|I=-1|P=c476f543|N=5f941696|U=ff16e645|C=90630e9c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.218|-|1|0|71.0832616,-1.56744123,58.1304689|0,0.673538781,0,0.739151886|1,1,1|V=336|I=-1|P=ed5ae45c|N=29214cc0|U=36d6e645|C=7d99f645|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.219|-|1|0|66.8174087,-1.55069838,32.3042225|0,0.0712776387,0,0.997456514|1,1,1|V=336|I=-1|P=0edd6d27|N=1596d6ce|U=63f6e645|C=14d050d2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.220|-|1|0|68.7939215,-1.2939177,47.1709317|0,0.764357374,0,0.644792839|1,1,1|V=336|I=-1|P=3e4a0717|N=2ce10868|U=80b6e645|C=69ee8670|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.221|-|1|0|67.8706744,-2.122,48.7881248|0,0.489841567,0,0.87181147|1,1,1|V=336|I=-1|P=f07f9282|N=d81721ca|U=6716e645|C=bc52ecef|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.222|-|1|0|69.9587274,-0.933620181,50.268897|0,0.993422108,0,0.114509888|1,1,1|V=336|I=-1|P=c28bcf04|N=8756f361|U=f656331a|C=0af269c8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.223|-|1|0|70.0893565,-2.122,58.0120102|0,0.406379711,0,0.91370429|1,1,1|V=336|I=-1|P=0a0e36f3|N=a193e95d|U=193e046a|C=b7e9a46d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.224|-|1|0|69.3893435,-1.0122974,47.9710741|0,-0.320089232,0,0.947387399|1,1,1|V=336|I=-1|P=2d5fc7df|N=6d115f0e|U=ff16e645|C=33ea2d76|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.225|-|1|0|70.7406754,-1.86089462,58.2322662|0,0.552658596,0,0.833407749|1,1,1|V=336|I=-1|P=88fd4b38|N=6be1aff9|U=36d6e645|C=bbc170eb|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.226|-|1|0|69.2694552,-0.84154739,45.9021866|0,0.940292753,0,0.340366769|1,1,1|V=336|I=-1|P=db1d8fba|N=04e23049|U=63f6e645|C=04a38723|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.227|-|1|0|69.5858026,0.211483161,39.2722363|0,0.490000089,0,0.871722383|1,1,1|V=336|I=-1|P=4f4e777e|N=325c06be|U=80b6e645|C=5788872a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.228|-|1|0|66.291221,-2.122,36.0231458|0,0.711885932,0,0.702295109|1,1,1|V=336|I=-1|P=81f3b3de|N=6033a2c6|U=6716e645|C=bc52ecef|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.229|-|1|0|69.0005514,-0.266633006,35.4980984|0,-0.68009041,0,0.733128253|1,1,1|V=336|I=-1|P=cf751b81|N=00dcc9d7|U=f656331a|C=6213e295|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.230|-|1|0|70.7845992,-1.06491093,53.8672237|0,-0.329446466,0,0.944174256|1,1,1|V=336|I=-1|P=11ac48ba|N=fb3a4562|U=193e046a|C=1c0151a2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.231|-|1|0|67.7668199,-1.18422202,37.2701285|0,0.0063883618,0,0.999979594|1,1,1|V=336|I=-1|P=0bf13983|N=8c7f5741|U=ff16e645|C=09ecd0ad|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.232|-|1|0|67.4404932,-1.50798671,33.1739061|0,-0.240811655,0,0.970571866|1,1,1|V=336|I=-1|P=2fbdebfd|N=38872b69|U=36d6e645|C=8b8e251c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.233|-|1|0|69.4790019,-1.75845197,52.5430234|0,-0.917413011,0,0.397936387|1,1,1|V=336|I=-1|P=acea3ef3|N=dc64ec04|U=63f6e645|C=ac19893f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.234|-|1|0|69.4090549,-2.03511322,53.3332388|0,0.0418757264,0,0.999122827|1,1,1|V=336|I=-1|P=b23f6c0e|N=eb7589f2|U=80b6e645|C=071428c7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.235|-|1|0|70.503452,0.00557005099,47.1299507|0,-0.499933372,0,0.866063868|1,1,1|V=336|I=-1|P=416715a4|N=b94ea9e4|U=6716e645|C=0123d3b2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.236|-|1|0|69.9321168,-2.0559391,55.3210274|0,-0.721573539,0,0.692337799|1,1,1|V=336|I=-1|P=f936750d|N=4bb3b6d6|U=f656331a|C=49be122f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.237|-|1|0|67.2562536,-2.122,44.6333585|0,0.635544967,0,0.772063854|1,1,1|V=336|I=-1|P=eb606efa|N=2734afd0|U=193e046a|C=d7c4d25e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.238|-|1|0|68.6204697,-0.714713649,41.4186366|0,-0.918982695,0,0.394297866|1,1,1|V=336|I=-1|P=18877d05|N=21990c6e|U=ff16e645|C=4ffbb7be|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.239|-|1|0|70.1480969,-2.122,58.165632|0,0.954519607,0,0.29814815|1,1,1|V=336|I=-1|P=b6b03899|N=8cedfc6b|U=36d6e645|C=3b9673a7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.240|-|1|0|68.5420065,-0.604992643,37.6786053|0,-0.961608187,0,0.274426118|1,1,1|V=336|I=-1|P=c4778ce0|N=cd6b70d6|U=63f6e645|C=a491adc1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.241|-|1|0|68.1957003,-2.122,49.2558244|0,-0.301428246,0,0.953488863|1,1,1|V=336|I=-1|P=45a05c37|N=459aefb8|U=80b6e645|C=e23836bd|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.242|-|1|0|67.8913829,-1.24387922,30.8640974|0,-0.940311591,0,0.340314724|1,1,1|V=336|I=-1|P=f7475efb|N=0f635ad3|U=6716e645|C=c9e1b301|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.243|-|1|0|69.0640831,-2.122,54.0572708|0,-0.214629709,0,0.976695494|1,1,1|V=336|I=-1|P=f1c6d8db|N=ec07d35d|U=f656331a|C=0af269c8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.244|-|1|0|69.7363502,-1.8061325,53.4534397|0,-0.990110064,0,0.140292767|1,1,1|V=336|I=-1|P=90e5ab15|N=99baed35|U=193e046a|C=d8b118de|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.245|-|1|0|69.9029464,1.03666307,32.3612916|0,-0.666312141,0,0.745672939|1,1,1|V=336|I=-1|P=3535cbce|N=9a4105f8|U=ff16e645|C=80f08d26|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.246|-|1|0|66.9411086,-1.87287642,39.6270503|0,-0.111718255,0,0.993739921|1,1,1|V=336|I=-1|P=ede3ecef|N=eda019a5|U=36d6e645|C=3b2155e0|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.247|-|1|0|67.3335887,-1.09771456,32.3969833|0,0.738144971,0,0.674642129|1,1,1|V=336|I=-1|P=00565aa6|N=994d63df|U=63f6e645|C=26a8fd37|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.248|-|1|0|67.3842797,-1.57043983,36.4353594|0,-0.847184101,0,0.531299443|1,1,1|V=336|I=-1|P=2342c791|N=bafb6c4c|U=80b6e645|C=52402a70|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.249|-|1|0|71.0648585,-1.81987983,59.3403801|0,0.91659198,0,0.399823889|1,1,1|V=336|I=-1|P=54d71c22|N=b9b66a88|U=6716e645|C=3d2fe079|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.250|-|1|0|69.8514609,-1.74839488,53.7166746|0,0.437822951,0,0.899061212|1,1,1|V=336|I=-1|P=57f0e2af|N=985752eb|U=f656331a|C=03d56b09|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.251|-|1|0|71.1225976,-1.68726473,58.6700936|0,-0.974606637,0,0.223923876|1,1,1|V=336|I=-1|P=c3a5b1fa|N=708da928|U=193e046a|C=be37be9f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.252|-|1|0|69.7348773,0.342165752,38.9084255|0,0.757873676,0,0.652401327|1,1,1|V=336|I=-1|P=4eb5c775|N=cff4c990|U=ff16e645|C=a9daa5c0|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.253|-|1|0|69.6874285,-1.29227416,50.7144982|0,-0.838381498,0,0.545083906|1,1,1|V=336|I=-1|P=293ec3ba|N=81139282|U=36d6e645|C=22d4b7c2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.254|-|1|0|69.0167691,-2.122,53.5311109|0,0.912230881,0,0.409676482|1,1,1|V=336|I=-1|P=5d4a66fd|N=5e99e026|U=63f6e645|C=ac19893f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.255|-|1|0|67.4664579,-2.122,47.8258739|0,0.996416001,0,0.0845881315|1,1,1|V=336|I=-1|P=a294a959|N=13721985|U=80b6e645|C=eb5bba42|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.256|quarry-sentinel-butte|1|1|16,-1.8,-68|0,0,0,1|1,1,1
r.3.256.0|-|1|0|=|V=54972|I=-1|P=5c2febd6|N=b7c8766b|U=13bec382|C=b1e71ea4|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.257|-|1|0|=|V=17598|I=-1|P=88d120d3|N=873c08c1|U=f34e7e55|C=d6806ca5|A=-|X=-|M=25f445b9|F=0,1,0,1
r.3.258|-|1|0|-48,-1.79,66|0,0,0,1|1,1,1|V=4440|I=-1|P=e6f79c1e|N=1bebe07f|U=c984fefd|C=78e10d2d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.259|-|1|0|-48,-1.79,70|0,0,0,1|1,1,1|V=4440|I=-1|P=e6f79c1e|N=1bebe07f|U=c984fefd|C=78e10d2d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.4|-|1|8|=
r.4.0|-|1|0|=|V=11160|I=-1|P=1ed4edaa|N=306ccff5|U=0373ab85|C=cf8d2065|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.4.1|-|1|0|=|V=1296|I=-1|P=2487532d|N=90d1d305|U=09332c25|C=48050b85|A=-|X=-|M=ec422f06|F=1,1,0,1
r.4.2|-|1|0|=|V=8976|I=-1|P=f2e09691|N=5f9d58a1|U=940b1ae1|C=4973d735|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.4.3|-|1|0|=|V=7488|I=-1|P=e60fc806|N=719aa9e9|U=6ae895fd|C=7bc97a3d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.4.4|-|1|0|=|V=540|I=-1|P=962cb757|N=58b93515|U=2d081b25|C=fd664a55|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.4.5|-|1|0|=|V=540|I=-1|P=3651a1a3|N=58b93515|U=2d081b25|C=01ebf371|A=-|X=-|M=d33b7c4a|F=1,1,0,1
r.4.6|-|1|0|=|V=4014|I=-1|P=c37860aa|N=cb151ead|U=-|C=c514a5a6|A=-|X=-|M=3ac9b326|F=1,1,0,1
r.4.7|-|1|0|=|V=6|I=-1|P=e2918304|N=9011a50d|U=8b3c62b5|C=ad11a50d|A=-|X=-|M=eb64e896|F=1,1,0,1
r.5|-|1|1|=
r.5.0|-|1|0|=|V=137148|I=-1|P=0cb2b7c6|N=2914f4c5|U=26302e75|C=c31cb26a|A=-|X=-|M=25f445b9|F=0,1,0,1
# floor-dry-grass-120
r|-|1|0|0,0.008,0|0,0,0,1|1,1,1|V=2401|I=13824|P=da6a05a7|N=6b6c5f6d|U=39dcbecd|C=7004e728|A=-|X=-|M=d9006d5a|F=0,1,0,1
# floor-packed-dirt-140
r|-|1|0|0,-0.002,0|0,0,0,1|1,1,1|V=4|I=6|P=887bf6b5|N=3f262745|U=86597165|C=-|A=-|X=-|M=56d272d3|F=0,1,0,1
# floor-packed-dirt-78
r|-|1|0|0,0.008,0|0,0,0,1|1,1,1|V=4|I=6|P=9b011da5|N=3f262745|U=d3b97165|C=-|A=-|X=-|M=56d272d3|F=0,1,0,1
# floor-dry-grass-140
r|-|1|0|0,-0.002,0|0,0,0,1|1,1,1|V=3249|I=18816|P=94f741f7|N=3b06c16d|U=98024b4d|C=b24b2aee|A=-|X=-|M=d9006d5a|F=0,1,0,1
# pads-0.65
r|spawn-pads|1|4|=
r.0|-|1|0|=|V=14400|I=-1|P=36927141|N=9de8fa65|U=a13b5035|C=008df045|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1|-|1|0|=|V=4620|I=-1|P=ca1f63de|N=68402dbd|U=f9864bd5|C=69c9f195|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.2|-|1|0|=|V=32400|I=-1|P=81e38a7d|N=8e0ad905|U=c4155b65|C=ce6e67c5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.3|-|1|0|=|V=4620|I=-1|P=27331f1a|N=c84f2335|U=f9864bd5|C=4366ccc1|A=-|X=-|M=d33b7c4a|F=1,1,0,1
# container-6
r|-|1|110|=
r.0|-|1|0|0,1.8,0|0,0.707106781,0,0.707106781|1,1,1|V=24|I=36|P=5d29fc65|N=79efde65|U=2c1b6d05|C=-|A=-|X=-|M=e741fb3d|F=1,1,0,1
r.1|-|1|0|-3.025,1.8,6.6|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.2|-|1|0|-3.025,1.8,6.05|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.3|-|1|0|-3.025,1.8,5.5|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.4|-|1|0|-3.025,1.8,4.95|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.5|-|1|0|-3.025,1.8,4.4|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.6|-|1|0|-3.025,1.8,3.85|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.7|-|1|0|-3.025,1.8,3.3|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.8|-|1|0|-3.025,1.8,2.75|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.9|-|1|0|-3.025,1.8,2.2|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.10|-|1|0|-3.025,1.8,1.65|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.11|-|1|0|-3.025,1.8,1.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.12|-|1|0|-3.025,1.8,0.55|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.13|-|1|0|-3.025,1.8,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.14|-|1|0|-3.025,1.8,-0.55|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.15|-|1|0|-3.025,1.8,-1.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.16|-|1|0|-3.025,1.8,-1.65|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.17|-|1|0|-3.025,1.8,-2.2|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.18|-|1|0|-3.025,1.8,-2.75|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.19|-|1|0|-3.025,1.8,-3.3|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.20|-|1|0|-3.025,1.8,-3.85|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.21|-|1|0|-3.025,1.8,-4.4|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.22|-|1|0|-3.025,1.8,-4.95|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.23|-|1|0|-3.025,1.8,-5.5|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.24|-|1|0|-3.025,1.8,-6.05|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.25|-|1|0|-3.025,1.8,-6.6|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.26|-|1|0|-3,0.12,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=487f2499|N=04279eb5|U=a4a42665|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.27|-|1|0|0,0.12,7|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=9e73bba5|N=04279eb5|U=8d03b69d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.28|-|1|0|-3,3.48,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=487f2499|N=04279eb5|U=a4a42665|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.29|-|1|0|0,3.48,7|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=9e73bba5|N=04279eb5|U=8d03b69d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.30|-|1|0|-2.87,1.84,6.9|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.31|-|1|0|2.87,1.84,6.9|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.32|-|1|0|-1.5,1.8,7.025|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ce885019|N=04279eb5|U=1d7c9c7d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.33|-|1|0|-1.5,1.8,7.08|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.34|-|1|0|-1.5,1.15,7.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.35|-|1|0|1.5,1.8,7.025|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ce885019|N=04279eb5|U=1d7c9c7d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.36|-|1|0|1.5,1.8,7.08|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.37|-|1|0|1.5,1.15,7.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.38|-|1|0|-3.09,2.34,3.78|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=f8fa3941|N=04279eb5|U=a2ff036d|C=-|A=-|X=-|M=8e58fabc|F=1,1,0,1
r.39|-|1|0|-3.12,2.34,4.28|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.40|-|1|0|-3.12,2.34,3.98|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.41|-|1|0|-3.12,2.34,3.68|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.42|-|1|0|-3.12,2.34,3.38|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.43|-|1|0|3.025,1.8,6.6|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.44|-|1|0|3.025,1.8,6.05|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.45|-|1|0|3.025,1.8,5.5|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.46|-|1|0|3.025,1.8,4.95|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.47|-|1|0|3.025,1.8,4.4|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.48|-|1|0|3.025,1.8,3.85|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.49|-|1|0|3.025,1.8,3.3|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.50|-|1|0|3.025,1.8,2.75|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.51|-|1|0|3.025,1.8,2.2|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.52|-|1|0|3.025,1.8,1.65|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.53|-|1|0|3.025,1.8,1.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.54|-|1|0|3.025,1.8,0.55|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.55|-|1|0|3.025,1.8,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.56|-|1|0|3.025,1.8,-0.55|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.57|-|1|0|3.025,1.8,-1.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.58|-|1|0|3.025,1.8,-1.65|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.59|-|1|0|3.025,1.8,-2.2|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.60|-|1|0|3.025,1.8,-2.75|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.61|-|1|0|3.025,1.8,-3.3|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.62|-|1|0|3.025,1.8,-3.85|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.63|-|1|0|3.025,1.8,-4.4|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.64|-|1|0|3.025,1.8,-4.95|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.65|-|1|0|3.025,1.8,-5.5|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.66|-|1|0|3.025,1.8,-6.05|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.67|-|1|0|3.025,1.8,-6.6|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.68|-|1|0|3,0.12,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=487f2499|N=04279eb5|U=a4a42665|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.69|-|1|0|0,0.12,-7|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=9e73bba5|N=04279eb5|U=8d03b69d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.70|-|1|0|3,3.48,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=487f2499|N=04279eb5|U=a4a42665|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.71|-|1|0|0,3.48,-7|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=9e73bba5|N=04279eb5|U=8d03b69d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.72|-|1|0|-2.87,1.84,-6.9|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.73|-|1|0|2.87,1.84,-6.9|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.74|-|1|0|-1.5,1.8,-7.025|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ce885019|N=04279eb5|U=1d7c9c7d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.75|-|1|0|-1.5,1.8,-7.08|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.76|-|1|0|-1.5,1.15,-7.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.77|-|1|0|1.5,1.8,-7.025|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=ce885019|N=04279eb5|U=1d7c9c7d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.78|-|1|0|1.5,1.8,-7.08|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.79|-|1|0|1.5,1.15,-7.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.80|-|1|0|3.09,2.34,3.78|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=f8fa3941|N=04279eb5|U=a2ff036d|C=-|A=-|X=-|M=8e58fabc|F=1,1,0,1
r.81|-|1|0|3.12,2.34,4.28|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.82|-|1|0|3.12,2.34,3.98|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.83|-|1|0|3.12,2.34,3.68|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.84|-|1|0|3.12,2.34,3.38|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.85|-|1|0|0,3.615,6.6|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.86|-|1|0|0,3.615,6.05|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.87|-|1|0|0,3.615,5.5|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.88|-|1|0|0,3.615,4.95|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.89|-|1|0|0,3.615,4.4|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.90|-|1|0|0,3.615,3.85|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.91|-|1|0|0,3.615,3.3|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.92|-|1|0|0,3.615,2.75|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.93|-|1|0|0,3.615,2.2|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.94|-|1|0|0,3.615,1.65|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.95|-|1|0|0,3.615,1.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.96|-|1|0|0,3.615,0.55|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.97|-|1|0|0,3.615,0|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.98|-|1|0|0,3.615,-0.55|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.99|-|1|0|0,3.615,-1.1|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.100|-|1|0|0,3.615,-1.65|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.101|-|1|0|0,3.615,-2.2|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.102|-|1|0|0,3.615,-2.75|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.103|-|1|0|0,3.615,-3.3|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.104|-|1|0|0,3.615,-3.85|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.105|-|1|0|0,3.615,-4.4|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.106|-|1|0|0,3.615,-4.95|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.107|-|1|0|0,3.615,-5.5|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.108|-|1|0|0,3.615,-6.05|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
r.109|-|1|0|0,3.615,-6.6|0,0.707106781,0,0.707106781|1,1,1|V=324|I=-1|P=71f6c439|N=04279eb5|U=7b1687ed|C=-|A=-|X=-|M=433cca79|F=1,1,0,1
# container-12
r|-|1|99|=
r.0|-|1|0|0,1.8,0|0,0,0,1|1,1,1|V=24|I=36|P=8fe9fc65|N=79efde65|U=db1b6d05|C=-|A=-|X=-|M=9e0aad6b|F=1,1,0,1
r.1|-|1|0|-5.6,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.2|-|1|0|-5.05,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.3|-|1|0|-4.5,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.4|-|1|0|-3.95,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.5|-|1|0|-3.4,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.6|-|1|0|-2.85,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.7|-|1|0|-2.3,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.8|-|1|0|-1.75,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.9|-|1|0|-1.2,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.10|-|1|0|-0.65,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.11|-|1|0|-0.1,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.12|-|1|0|0.45,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.13|-|1|0|1,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.14|-|1|0|1.55,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.15|-|1|0|2.1,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.16|-|1|0|2.65,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.17|-|1|0|3.2,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.18|-|1|0|3.75,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.19|-|1|0|4.3,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.20|-|1|0|4.85,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.21|-|1|0|5.4,1.8,-2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.22|-|1|0|0,0.12,-2.5|0,0,0,1|1,1,1|V=324|I=-1|P=6abf2499|N=04279eb5|U=3ad6455d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.23|-|1|0|-6,0.12,0|0,0,0,1|1,1,1|V=324|I=-1|P=8c33bba5|N=04279eb5|U=85b6a1c5|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.24|-|1|0|0,3.48,-2.5|0,0,0,1|1,1,1|V=324|I=-1|P=6abf2499|N=04279eb5|U=3ad6455d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.25|-|1|0|-6,3.48,0|0,0,0,1|1,1,1|V=324|I=-1|P=8c33bba5|N=04279eb5|U=85b6a1c5|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.26|-|1|0|-5.9,1.84,-2.37|0,0,0,1|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.27|-|1|0|-5.9,1.84,2.37|0,0,0,1|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.28|-|1|0|-6.025,1.8,-1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f6c85019|N=04279eb5|U=f1a26e9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.29|-|1|0|-6.08,1.8,-1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.30|-|1|0|-6.1,1.15,-1.25|0,0,0,1|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.31|-|1|0|-6.025,1.8,1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f6c85019|N=04279eb5|U=f1a26e9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.32|-|1|0|-6.08,1.8,1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.33|-|1|0|-6.1,1.15,1.25|0,0,0,1|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.34|-|1|0|-3.24,2.34,-2.59|0,0,0,1|1,1,1|V=324|I=-1|P=f8fa3941|N=04279eb5|U=a2ff036d|C=-|A=-|X=-|M=8e58fabc|F=1,1,0,1
r.35|-|1|0|-3.74,2.34,-2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.36|-|1|0|-3.44,2.34,-2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.37|-|1|0|-3.14,2.34,-2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.38|-|1|0|-2.84,2.34,-2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.39|-|1|0|-5.6,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.40|-|1|0|-5.05,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.41|-|1|0|-4.5,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.42|-|1|0|-3.95,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.43|-|1|0|-3.4,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.44|-|1|0|-2.85,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.45|-|1|0|-2.3,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.46|-|1|0|-1.75,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.47|-|1|0|-1.2,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.48|-|1|0|-0.65,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.49|-|1|0|-0.1,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.50|-|1|0|0.45,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.51|-|1|0|1,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.52|-|1|0|1.55,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.53|-|1|0|2.1,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.54|-|1|0|2.65,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.55|-|1|0|3.2,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.56|-|1|0|3.75,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.57|-|1|0|4.3,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.58|-|1|0|4.85,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.59|-|1|0|5.4,1.8,2.525|0,0,0,1|1,1,1|V=324|I=-1|P=3bba0745|N=04279eb5|U=27c0f14d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.60|-|1|0|0,0.12,2.5|0,0,0,1|1,1,1|V=324|I=-1|P=6abf2499|N=04279eb5|U=3ad6455d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.61|-|1|0|6,0.12,0|0,0,0,1|1,1,1|V=324|I=-1|P=8c33bba5|N=04279eb5|U=85b6a1c5|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.62|-|1|0|0,3.48,2.5|0,0,0,1|1,1,1|V=324|I=-1|P=6abf2499|N=04279eb5|U=3ad6455d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.63|-|1|0|6,3.48,0|0,0,0,1|1,1,1|V=324|I=-1|P=8c33bba5|N=04279eb5|U=85b6a1c5|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.64|-|1|0|5.9,1.84,-2.37|0,0,0,1|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.65|-|1|0|5.9,1.84,2.37|0,0,0,1|1,1,1|V=324|I=-1|P=ccec5471|N=04279eb5|U=cb83550d|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.66|-|1|0|6.025,1.8,-1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f6c85019|N=04279eb5|U=f1a26e9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.67|-|1|0|6.08,1.8,-1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.68|-|1|0|6.1,1.15,-1.25|0,0,0,1|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.69|-|1|0|6.025,1.8,1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f6c85019|N=04279eb5|U=f1a26e9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.70|-|1|0|6.08,1.8,1.25|0,0,0,1|1,1,1|V=324|I=-1|P=f2fe9805|N=04279eb5|U=a8037ddd|C=-|A=-|X=-|M=d2dc99d7|F=1,1,0,1
r.71|-|1|0|6.1,1.15,1.25|0,0,0,1|1,1,1|V=324|I=-1|P=c23f2651|N=04279eb5|U=7845b39d|C=-|A=-|X=-|M=abc8fc47|F=1,1,0,1
r.72|-|1|0|-3.24,2.34,2.59|0,0,0,1|1,1,1|V=324|I=-1|P=f8fa3941|N=04279eb5|U=a2ff036d|C=-|A=-|X=-|M=8e58fabc|F=1,1,0,1
r.73|-|1|0|-3.74,2.34,2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.74|-|1|0|-3.44,2.34,2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.75|-|1|0|-3.14,2.34,2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.76|-|1|0|-2.84,2.34,2.62|0,0,0,1|1,1,1|V=324|I=-1|P=a9a74255|N=04279eb5|U=0df24d9d|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.77|-|1|0|-5.6,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.78|-|1|0|-5.05,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.79|-|1|0|-4.5,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.80|-|1|0|-3.95,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.81|-|1|0|-3.4,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.82|-|1|0|-2.85,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.83|-|1|0|-2.3,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.84|-|1|0|-1.75,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.85|-|1|0|-1.2,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.86|-|1|0|-0.65,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.87|-|1|0|-0.1,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.88|-|1|0|0.45,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.89|-|1|0|1,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.90|-|1|0|1.55,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.91|-|1|0|2.1,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.92|-|1|0|2.65,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.93|-|1|0|3.2,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.94|-|1|0|3.75,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.95|-|1|0|4.3,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.96|-|1|0|4.85,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.97|-|1|0|5.4,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
r.98|-|1|0|5.95,3.615,0|0,0,0,1|1,1,1|V=324|I=-1|P=78f6c439|N=04279eb5|U=c338a545|C=-|A=-|X=-|M=e0d25643|F=1,1,0,1
# footing
r|-|1|0|=|V=48|I=192|P=cdb819fc|N=78070a26|U=160bd282|C=9f1b1b35|A=da55ff85|X=-|M=7f8ad734|F=0,1,0,1
# soil
accum|ee755f57
band-1000-1003|e38bfa6c
band-0-2|7f112245
full|7e5daaa2
"#;
