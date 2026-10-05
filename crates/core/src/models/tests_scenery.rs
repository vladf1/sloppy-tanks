//! Scenery compared with the TypeScript/Three.js implementation. The expected
//! dump below was printed by a Node script that built `VillageScenery`,
//! `HarborScenery`, `QuarryScenery`, `createArenaFloor`, `createSpawnPads`,
//! `cargoStack`, `shippingContainer` and `sandstoneFooting` with Three r185 and
//! walked each tree depth-first. Each line is one node:
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
use super::*;
use crate::geometry::VERTEX_ALPHA;
use crate::scene::{Effect, Material, Node, Shading, Side, TextureRef, TextureSource};
use crate::sim::quarry_rock_shape::quarry_rock_shape;

fn fnv(words: impl IntoIterator<Item = u32>) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for word in words {
        h ^= word;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

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
    let side = match m.side {
        Side::Front => 0,
        Side::Back => 1,
        Side::Double => 2,
    };
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
        side.to_string(),
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

/// Shorten a full TypeScript dump line like `dump(.., full = false)`.
fn compact(line: &str) -> String {
    let mut fields: Vec<String> = line.split('|').map(str::to_string).collect();
    if fields.len() >= 7 && fields[4..7].join("|") == "0,0,0|0,0,0,1|1,1,1" {
        fields.splice(4..7, ["=".to_string()]);
    }
    for field in &mut fields {
        if let Some(summary) = field.strip_prefix("M=")
            && summary.contains(',')
        {
            *field = format!("M={}", summary_hash(summary));
        }
    }
    fields.join("|")
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
    sections.push(("quarry".into(), QuarryScenery::new().root));
    for (kind, extent, y) in [
        (GroundKind::DryGrass, 120.0, 0.008),
        (GroundKind::PackedDirt, 140.0, -0.002),
        (GroundKind::PackedDirt, 78.0, 0.008),
        (GroundKind::DryGrass, 140.0, -0.002),
    ] {
        sections.push((
            format!("floor-{}-{extent}", kind.as_str()),
            custom_floor(kind, Some(extent), y),
        ));
    }
    sections.push(("pads-0.65".into(), custom_spawn_pads(0.65)));
    let crates = [
        CrateShape {
            x: 4.0,
            z: -20.0,
            w: 3.0,
            d: 3.0,
            h: 2.6,
            color: 0xb88b53,
        },
        CrateShape {
            x: -31.2,
            z: 35.4,
            w: 2.4,
            d: 2.8,
            h: 2.1,
            color: 0xa18e6f,
        },
    ];
    for (i, c) in crates.into_iter().enumerate() {
        for stage in 0..3 {
            let mut group = Node::group("");
            cargo_stack(&mut group, c, stage);
            sections.push((format!("cargo-{i}-{stage}"), group));
        }
    }
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
    let reference = reference();
    let mut expected: Vec<(String, Vec<String>)> = Vec::new();
    for line in reference.lines() {
        if let Some(name) = line.strip_prefix("# ") {
            expected.push((name.to_string(), Vec::new()));
        } else if let Some((_, lines)) = expected.last_mut() {
            lines.push(compact(line));
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

fn reference() -> String {
    std::env::var("SCENERY_REFERENCE")
        .map(|file| std::fs::read_to_string(file).unwrap())
        .unwrap_or_else(|_| EXPECTED.to_string())
}

/// A `key|hash` line of the reference's `soil` section.
fn soil_hash(key: &str) -> String {
    reference()
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
        custom_floor(GroundKind::DryGrass, Some(120.0), 0.008),
        custom_floor(GroundKind::PackedDirt, Some(140.0), -0.002),
    ];
    let pads = custom_spawn_pads(0.65);
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
r.1.1|-|1|0|=|V=1080|I=-1|P=ad72368d|N=907dad65|U=3d1a86a5|C=f0242571|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1.2|-|1|0|=|V=4320|I=-1|P=3402eb4d|N=3a5485c7|U=f11e7ef9|C=74038d15|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.1.3|-|1|0|=|V=2340|I=-1|P=a43b0b8d|N=e273bd15|U=cd62751d|C=a2bd163d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.1.4|-|1|0|=|V=288|I=-1|P=7d667a89|N=f8434b85|U=dea98b25|C=ee02c765|A=-|X=-|M=e325459e|F=1,1,0,1
r.1.5|-|1|0|=|V=132|I=-1|P=910b28df|N=58057c47|U=8fe9af65|C=ad793e39|A=-|X=-|M=86f613cb|F=1,1,0,1
r.1.6|-|1|0|=|V=72|I=-1|P=f73b2f7d|N=a8a41ba5|U=155c3305|C=634ef91d|A=-|X=-|M=e19d6c00|F=1,1,0,1
r.1.7|-|1|0|=|V=36|I=-1|P=aa34c4fd|N=f87a9e35|U=5aefde65|C=bc036ab1|A=-|X=-|M=871e65c5|F=1,1,0,1
r.2|quarry-haul-truck|1|13|69,-1.75,18|0,0.767798663,0,0.6406912|1,1,1
r.2.0|-|1|0|=|V=108|I=-1|P=6ee8b395|N=a0200615|U=e98a391d|C=386a8d6d|A=-|X=-|M=90c71f94|F=1,1,0,1
r.2.1|-|1|0|=|V=1152|I=-1|P=8cb47479|N=9d4c5865|U=f06a8da5|C=63048a45|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.2.2|-|1|0|=|V=1440|I=-1|P=f8d9097f|N=cc98d025|U=86363c95|C=3967fec5|A=-|X=-|M=e325459e|F=1,1,0,1
r.2.3|-|1|0|=|V=180|I=-1|P=dc294259|N=d1e3e1f5|U=9f60a8e5|C=41e9ad7d|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.2.4|-|1|0|=|V=36|I=-1|P=8e69a04d|N=f87a9e35|U=5aefde65|C=991f0285|A=-|X=-|M=a01f6c8d|F=1,1,0,1
r.2.5|-|1|0|=|V=72|I=-1|P=e2070e0d|N=a8a41ba5|U=155c3305|C=7ff0f6dd|A=-|X=-|M=86f613cb|F=1,1,0,1
r.2.6|-|1|0|=|V=72|I=-1|P=e1ee83d5|N=a8a41ba5|U=155c3305|C=4bd832ad|A=-|X=-|M=de456449|F=1,1,0,1
r.2.7|-|1|0|=|V=36|I=-1|P=3939b37d|N=f87a9e35|U=5aefde65|C=2a41db99|A=-|X=-|M=e19d6c00|F=1,1,0,1
r.2.8|-|1|0|-0.6,3.8,1|0,0,0,1|1,1,1|V=624|I=-1|P=8023a6af|N=f46e739c|U=8224aa91|C=cea03297|A=-|X=-|M=25f445b9|F=1,1,0,1
r.2.9|-|1|0|1.2,3.8,-1|0,0,0,1|1,1,1|V=624|I=-1|P=29a9058f|N=12c69cd4|U=3604846c|C=bbd3320c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.2.10|-|1|0|3,3.8,1|0,0,0,1|1,1,1|V=624|I=-1|P=02b7588b|N=2cdc3273|U=a73a1bde|C=e5c96379|A=-|X=-|M=25f445b9|F=1,1,0,1
r.2.11|-|1|0|-0.6,3.8,-1|0,0,0,1|1,1,1|V=624|I=-1|P=ca11b7d9|N=e666e78b|U=f1717ece|C=c521204d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.2.12|-|1|0|1.2,3.8,1|0,0,0,1|1,1,1|V=624|I=-1|P=8023a6af|N=f46e739c|U=8224aa91|C=cea03297|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3|-|1|4|=
r.3.0|quarry-sentinel-butte|1|1|16,-1.8,-68|0,0,0,1|1,1,1
r.3.0.0|-|1|0|=|V=3744|I=-1|P=481b00ed|N=6546b63d|U=5ec5b4ff|C=7516217c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1|-|1|0|=|V=411408|I=-1|P=61836a07|N=c35f596c|U=7f029d57|C=460cb5a7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2|-|1|0|=|V=37866|I=-1|P=7ca2f9eb|N=1180e206|U=fa6bc976|C=2709c014|A=-|X=-|M=e19d6c00|F=1,1,0,1
r.3.3|-|1|0|=|V=16056|I=-1|P=c10327d6|N=2142ce5d|U=34ba8594|C=c04237e5|A=-|X=-|M=e19d6c00|F=1,1,0,1
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
r.5.0|-|1|0|=|V=56940|I=-1|P=ac9ceeae|N=4d17238c|U=d2921c68|C=50dbc2f1|A=-|X=-|M=25f445b9|F=0,1,0,1
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
# cargo-0-0
r|-|1|11|=
r.0|-|1|0|0,1.41,0|0,0,0,1|1,1,1|V=24|I=36|P=3b2ea245|N=79efde65|U=e94dff85|C=-|A=-|X=-|M=102350ad|F=1,1,0,1
r.1|-|1|0|-1.02,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=48f76a65|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.2|-|1|0|-1.02,1.43,0|0,0,0,1|1,1,1|V=24|I=36|P=d7fce6c5|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.3|-|1|0|1.02,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=48f76a65|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.4|-|1|0|1.02,1.43,0|0,0,0,1|1,1,1|V=24|I=36|P=d7fce6c5|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.5|-|1|0|0,0.42,-1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.6|-|1|0|0,2.4,-1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.7|-|1|0|0,1.3,-1.58|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=4cd594fd|N=79efde65|U=98b7ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.8|-|1|0|0,0.42,1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.9|-|1|0|0,2.4,1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.10|-|1|0|0,1.3,1.58|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=4cd594fd|N=79efde65|U=98b7ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
# cargo-0-1
r|-|1|23|=
r.0|-|1|0|0,1.41,0|0,0,0,1|1,1,1|V=24|I=36|P=3b2ea245|N=79efde65|U=e94dff85|C=-|A=-|X=-|M=102350ad|F=1,1,0,1
r.1|-|1|0|-1.02,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=48f76a65|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.2|-|1|0|-1.02,1.43,0|0,0,0,1|1,1,1|V=24|I=36|P=d7fce6c5|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.3|-|1|0|1.02,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=48f76a65|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.4|-|1|0|1.02,1.43,0|0,0,0,1|1,1,1|V=24|I=36|P=d7fce6c5|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.5|-|1|0|0,0.42,-1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.6|-|1|0|0,2.4,-1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.7|-|1|0|0,1.3,-1.58|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=4cd594fd|N=79efde65|U=98b7ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.8|-|1|0|0,0.42,1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.9|-|1|0|0,2.4,1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.10|-|1|0|0,1.3,1.58|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=4cd594fd|N=79efde65|U=98b7ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.11|cargo-split|1|0|-0.0461959382,2.625,-0.0929750799|-0.704336429,0.0625315499,0.0625315499,0.704336429|0.146774024,2.27399548,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.12|cargo-split|1|0|-0.0461959382,2.643,-0.0929750799|-0.704336429,0.0625315499,0.0625315499,0.704336429|0.0772494862,2.27399548,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.13|cargo-split|1|0|0.628597986,2.625,0.169667852|-0.693873767,0.136158716,0.136158716,0.693873767|0.181559381,0.914894338,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.14|cargo-split|1|0|0.628597986,2.643,0.169667852|-0.693873767,0.136158716,0.136158716,0.693873767|0.0955575689,0.914894338,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.15|cargo-split|1|0|-0.447834071,1.43230653,-1.525|-0.109375558,0.994000497,0,0|0.203968912,1.35215356,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.16|cargo-split|1|0|-0.447834071,1.43230653,-1.543|-0.109375558,0.994000497,0,0|0.107352059,1.35215356,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.17|cargo-split|1|0|-1.525,1.35609173,-0.141938028|0.00642662989,-0.707077576,-0.00642662989,0.707077576|0.176133028,1.72996897,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.18|cargo-split|1|0|-1.543,1.35609173,-0.141938028|0.00642662989,-0.707077576,-0.00642662989,0.707077576|0.0927015935,1.72996897,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.19|cargo-split|1|0|0.346907667,1.2882588,1.525|0,0,-0.0136728225,0.999906523|0.138935426,1.74877222,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.20|cargo-split|1|0|0.346907667,1.2882588,1.543|0,0,-0.0136728225,0.999906523|0.0731239084,1.74877222,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.21|cargo-split|1|0|1.525,1.44268892,0.232364599|-0.0680133743,0.703828233,-0.0680133743,0.703828233|0.146440736,1.38623593,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.22|cargo-split|1|0|1.543,1.44268892,0.232364599|-0.0680133743,0.703828233,-0.0680133743,0.703828233|0.0770740715,1.38623593,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
# cargo-0-2
r|-|1|32|=
r.0|-|1|0|0,1.41,0|0,0,0,1|1,1,1|V=24|I=36|P=3b2ea245|N=79efde65|U=e94dff85|C=-|A=-|X=-|M=102350ad|F=1,1,0,1
r.1|-|1|0|-1.02,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=48f76a65|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.2|-|1|0|-1.02,1.4,-1.52|0,0,0,1|1,1,1|V=24|I=36|P=0ae97385|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.3|-|1|0|-1.02,2.66,-1.02|0,0,0,1|1,1,1|V=24|I=36|P=a1dd5a7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.4|-|1|0|-1.02,1.4,1.52|0,0,0,1|1,1,1|V=24|I=36|P=0ae97385|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.5|-|1|0|-1.02,2.66,1.02|0,0,0,1|1,1,1|V=24|I=36|P=a1dd5a7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.6|-|1|0|-1.02,2.765,0.3|0.140821256,0,0,0.990035037|1,1,1|V=24|I=36|P=dca76405|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.7|-|1|0|1.02,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=48f76a65|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.8|-|1|0|1.02,1.43,0|0,0,0,1|1,1,1|V=24|I=36|P=d7fce6c5|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.9|-|1|0|0,0.42,-1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.10|-|1|0|0,2.4,-1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.11|-|1|0|0,1.3,-1.58|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=4cd594fd|N=79efde65|U=98b7ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.12|-|1|0|0,0.42,1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.13|-|1|0|0,2.4,1.54|0,0,0,1|1,1,1|V=24|I=36|P=5d7d71f5|N=79efde65|U=9245ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.14|-|1|0|0,1.3,1.58|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=4cd594fd|N=79efde65|U=98b7ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.15|cargo-split|1|0|-0.0461959382,2.625,-0.0929750799|-0.704336429,0.0625315499,0.0625315499,0.704336429|0.262648253,2.27399548,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.16|cargo-split|1|0|-0.0461959382,2.643,-0.0929750799|-0.704336429,0.0625315499,0.0625315499,0.704336429|0.138235923,2.27399548,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.17|cargo-split|1|0|0.628597986,2.625,0.169667852|-0.693873767,0.136158716,0.136158716,0.693873767|0.324895734,0.914894338,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.18|cargo-split|1|0|0.628597986,2.643,0.169667852|-0.693873767,0.136158716,0.136158716,0.693873767|0.170997755,0.914894338,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.19|cargo-split|1|0|-0.447834071,1.43230653,-1.525|-0.109375558,0.994000497,0,0|0.364997001,1.35215356,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.20|cargo-split|1|0|-0.447834071,1.43230653,-1.543|-0.109375558,0.994000497,0,0|0.192103685,1.35215356,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.21|cargo-split|1|0|-1.525,1.35609173,-0.141938028|0.00642662989,-0.707077576,-0.00642662989,0.707077576|0.315185418,1.72996897,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.22|cargo-split|1|0|-1.543,1.35609173,-0.141938028|0.00642662989,-0.707077576,-0.00642662989,0.707077576|0.165887062,1.72996897,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.23|cargo-split|1|0|0.346907667,1.2882588,1.525|0,0,-0.0136728225,0.999906523|0.248621289,1.74877222,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.24|cargo-split|1|0|0.346907667,1.2882588,1.543|0,0,-0.0136728225,0.999906523|0.13085331,1.74877222,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.25|cargo-split|1|0|1.525,1.44268892,0.232364599|-0.0680133743,0.703828233,-0.0680133743,0.703828233|0.262051843,1.38623593,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.26|cargo-split|1|0|1.543,1.44268892,0.232364599|-0.0680133743,0.703828233,-0.0680133743,0.703828233|0.137922023,1.38623593,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.27|-|1|0|-0.304812651,2.82,-0.460890768|-0.0444797145,0.08579531,-0.0456185288,0.994273438|1,1,1|V=24|I=36|P=604dc60d|N=79efde65|U=f6d5ff85|C=-|A=-|X=-|M=102350ad|F=1,1,0,1
r.28|-|1|0|0.421176475,2.82,-0.474741656|0.0126447745,0.021815825,0.0496963967,0.998446016|1,1,1|V=24|I=36|P=604dc60d|N=79efde65|U=f6d5ff85|C=-|A=-|X=-|M=102350ad|F=1,1,0,1
r.29|-|1|0|-0.270438863,2.76,0.65459262|-0.00176379432,-0.015042247,0.116444825,0.99308168|1,1,1|V=24|I=36|P=9794ceed|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=6525f785|F=1,1,0,1
r.30|-|1|0|0.605188143,2.76,0.687205668|0.00478302832,0.173244887,0.0271803305,0.984492032|1,1,1|V=24|I=36|P=9794ceed|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=6525f785|F=1,1,0,1
r.31|-|1|0|-0.552799147,2.76,-0.233159936|-0.0325650687,0.129435137,-0.241807023,0.961101152|1,1,1|V=24|I=36|P=9794ceed|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=6525f785|F=1,1,0,1
# cargo-1-0
r|-|1|11|=
r.0|-|1|0|0,1.16,0|0,0,0,1|1,1,1|V=24|I=36|P=c56ac195|N=79efde65|U=afedff85|C=-|A=-|X=-|M=64c6279c|F=1,1,0,1
r.1|-|1|0|-0.816,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=1356e27d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.2|-|1|0|-0.816,1.18,0|0,0,0,1|1,1,1|V=24|I=36|P=419aac7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.3|-|1|0|0.816,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=1356e27d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.4|-|1|0|0.816,1.18,0|0,0,0,1|1,1,1|V=24|I=36|P=419aac7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.5|-|1|0|0,0.42,-1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.6|-|1|0|0,1.9,-1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.7|-|1|0|0,1.05,-1.48|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=0538a1dd|N=79efde65|U=dd55ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.8|-|1|0|0,0.42,1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.9|-|1|0|0,1.9,1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.10|-|1|0|0,1.05,1.48|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=0538a1dd|N=79efde65|U=dd55ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
# cargo-1-1
r|-|1|23|=
r.0|-|1|0|0,1.16,0|0,0,0,1|1,1,1|V=24|I=36|P=c56ac195|N=79efde65|U=afedff85|C=-|A=-|X=-|M=64c6279c|F=1,1,0,1
r.1|-|1|0|-0.816,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=1356e27d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.2|-|1|0|-0.816,1.18,0|0,0,0,1|1,1,1|V=24|I=36|P=419aac7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.3|-|1|0|0.816,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=1356e27d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.4|-|1|0|0.816,1.18,0|0,0,0,1|1,1,1|V=24|I=36|P=419aac7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.5|-|1|0|0,0.42,-1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.6|-|1|0|0,1.9,-1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.7|-|1|0|0,1.05,-1.48|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=0538a1dd|N=79efde65|U=dd55ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.8|-|1|0|0,0.42,1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.9|-|1|0|0,1.9,1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.10|-|1|0|0,1.05,1.48|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=0538a1dd|N=79efde65|U=dd55ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.11|cargo-split|1|0|-0.310147792,2.125,0.0754038922|-0.706287305,0.0340329575,0.0340329575,0.706287305|0.21311372,2.07417123,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.12|cargo-split|1|0|-0.310147792,2.143,0.0754038922|-0.706287305,0.0340329575,0.0340329575,0.706287305|0.112165116,2.07417123,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.13|cargo-split|1|0|0.401162882,2.125,0.291938188|-0.696812813,-0.120216072,-0.120216072,0.696812813|0.16031793,0.878976243,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.14|cargo-split|1|0|0.401162882,2.143,0.291938188|-0.696812813,-0.120216072,-0.120216072,0.696812813|0.084377858,0.878976243,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.15|cargo-split|1|0|-0.302165813,1.04520832,-1.425|0.0226010812,0.999744563,0,0|0.207006727,1.15340818,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.16|cargo-split|1|0|-0.302165813,1.04520832,-1.443|0.0226010812,0.999744563,0,0|0.108950909,1.15340818,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.17|cargo-split|1|0|-1.225,1.15680111,0.548129597|0.0776695597,-0.702828172,-0.0776695597,0.702828172|0.156747474,1.39935409,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.18|cargo-split|1|0|-1.243,1.15680111,0.548129597|0.0776695597,-0.702828172,-0.0776695597,0.702828172|0.0824986705,1.39935409,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.19|cargo-split|1|0|-0.437658433,1.15010916,1.425|0,0,0.105185285,0.994452641|0.161238778,1.35778879,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.20|cargo-split|1|0|-0.437658433,1.15010916,1.443|0,0,0.105185285,0.994452641|0.0848625146,1.35778879,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.21|cargo-split|1|0|1.225,1.09541256,-0.55387503|-0.101077083,0.699845285,-0.101077083,0.699845285|0.15073506,1.23987317,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.22|cargo-split|1|0|1.243,1.09541256,-0.55387503|-0.101077083,0.699845285,-0.101077083,0.699845285|0.079334242,1.23987317,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
# cargo-1-2
r|-|1|32|=
r.0|-|1|0|0,1.16,0|0,0,0,1|1,1,1|V=24|I=36|P=c56ac195|N=79efde65|U=afedff85|C=-|A=-|X=-|M=64c6279c|F=1,1,0,1
r.1|-|1|0|-0.816,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=1356e27d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.2|-|1|0|-0.816,1.18,0|0,0,0,1|1,1,1|V=24|I=36|P=419aac7d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.3|-|1|0|0.816,0.11,0|0,0,0,1|1,1,1|V=24|I=36|P=1356e27d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=1884f23f|F=1,1,0,1
r.4|-|1|0|0.816,1.15,-1.42|0,0,0,1|1,1,1|V=24|I=36|P=c6e6e32d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.5|-|1|0|0.816,2.16,-0.952|0,0,0,1|1,1,1|V=24|I=36|P=39124205|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.6|-|1|0|0.816,1.15,1.42|0,0,0,1|1,1,1|V=24|I=36|P=c6e6e32d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.7|-|1|0|0.816,2.16,0.952|0,0,0,1|1,1,1|V=24|I=36|P=39124205|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.8|-|1|0|0.816,2.258,-0.28|-0.234596296,0,0,0.972092885|1,1,1|V=24|I=36|P=ba160625|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=4e3b725f|F=1,1,0,1
r.9|-|1|0|0,0.42,-1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.10|-|1|0|0,1.9,-1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.11|-|1|0|0,1.05,-1.48|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=0538a1dd|N=79efde65|U=dd55ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.12|-|1|0|0,0.42,1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.13|-|1|0|0,1.9,1.44|0,0,0,1|1,1,1|V=24|I=36|P=5054cb85|N=79efde65|U=62d5ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.14|-|1|0|0,1.05,1.48|0,0,0.285952225,0.958243876|1,1,1|V=24|I=36|P=0538a1dd|N=79efde65|U=dd55ff85|C=-|A=-|X=-|M=58afc730|F=1,1,0,1
r.15|cargo-split|1|0|-0.310147792,2.125,0.0754038922|-0.706287305,0.0340329575,0.0340329575,0.706287305|0.381361394,2.07417123,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.16|cargo-split|1|0|-0.310147792,2.143,0.0754038922|-0.706287305,0.0340329575,0.0340329575,0.706287305|0.200716523,2.07417123,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.17|cargo-split|1|0|0.401162882,2.125,0.291938188|-0.696812813,-0.120216072,-0.120216072,0.696812813|0.286884717,0.878976243,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.18|cargo-split|1|0|0.401162882,2.143,0.291938188|-0.696812813,-0.120216072,-0.120216072,0.696812813|0.150991956,0.878976243,1|V=11|I=27|P=b6a54c7c|N=1c05589f|U=57981cee|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.19|cargo-split|1|0|-0.302165813,1.04520832,-1.425|0.0226010812,0.999744563,0,0|0.37043309,1.15340818,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.20|cargo-split|1|0|-0.302165813,1.04520832,-1.443|0.0226010812,0.999744563,0,0|0.194964784,1.15340818,1|V=11|I=27|P=bc9707b4|N=1c05589f|U=14ea1866|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.21|cargo-split|1|0|-1.225,1.15680111,0.548129597|0.0776695597,-0.702828172,-0.0776695597,0.702828172|0.28049548,1.39935409,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.22|cargo-split|1|0|-1.243,1.15680111,0.548129597|0.0776695597,-0.702828172,-0.0776695597,0.702828172|0.1476292,1.39935409,1|V=11|I=27|P=0d7f60e6|N=1c05589f|U=7e160a6c|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.23|cargo-split|1|0|-0.437658433,1.15010916,1.425|0,0,0.105185285,0.994452641|0.288532549,1.35778879,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.24|cargo-split|1|0|-0.437658433,1.15010916,1.443|0,0,0.105185285,0.994452641|0.151859237,1.35778879,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.25|cargo-split|1|0|1.225,1.09541256,-0.55387503|-0.101077083,0.699845285,-0.101077083,0.699845285|0.269736423,1.23987317,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=07b40276|F=0,0,0,1
r.26|cargo-split|1|0|1.243,1.09541256,-0.55387503|-0.101077083,0.699845285,-0.101077083,0.699845285|0.141966538,1.23987317,1|V=11|I=27|P=c1c52cb0|N=1c05589f|U=fe9a4aac|C=-|A=-|X=-|M=439f30cc|F=0,0,0,1
r.27|-|1|0|-0.250323639,2.32,-0.312350834|-0.0158083183,-0.0439078966,-0.0435009279,0.997962857|1,1,1|V=24|I=36|P=29e5a92d|N=79efde65|U=ff5aab6d|C=-|A=-|X=-|M=64c6279c|F=1,1,0,1
r.28|-|1|0|0.381083038,2.32,-0.0271160477|-0.0134612888,-0.0610725237,0.0474560571,0.996913669|1,1,1|V=24|I=36|P=29e5a92d|N=79efde65|U=ff5aab6d|C=-|A=-|X=-|M=64c6279c|F=1,1,0,1
r.29|-|1|0|-0.317656598,2.26,-0.306540099|-0.0216808001,-0.189212363,0.111755954,0.975314939|1,1,1|V=24|I=36|P=9d117eed|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=6525f785|F=1,1,0,1
r.30|-|1|0|0.51899019,2.26,0.0359003627|-0.0140425373,0.106689854,-0.129736934,0.985691843|1,1,1|V=24|I=36|P=9d117eed|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=6525f785|F=1,1,0,1
r.31|-|1|0|-0.112623135,2.26,-0.258818614|-0.0174252075,0.292016724,-0.0569602486,0.954556507|1,1,1|V=24|I=36|P=9d117eed|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=6525f785|F=1,1,0,1
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
band-1000-1003|9bb9b217
band-0-2|f24eb5f3
full|63c76409
"#;
