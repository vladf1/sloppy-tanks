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
//! A few batches added since the port (Pine Village's sculpted ranges and the
//! pines standing on them, a farmstead batch and a quarry batch) are shaped with
//! `sin`, `exp` and `powf`, whose last bits differ between platform maths
//! libraries (macOS against Linux CI), so they are compared by their counts
//! without their float hashes ([`platform_math`]).
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

/// Subtrees only one side builds: Three's reflector targets, and the farmhouse,
/// which is the village cottage (`house_model`, tested there) since the cottages
/// were redesigned.
fn skipped(section: &str, path: &str) -> bool {
    (section == "village" && (path.starts_with("r.4.1.") || path.starts_with(FARMHOUSE)))
        || (section == "harbor" && path.starts_with("r.0."))
}

/// The farmstead's farmhouse cottage in the village dump.
const FARMHOUSE: &str = "r.7.0.1.0";

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

/// Batches whose vertex and instance hashes depend on the platform's maths
/// library (see the module docs).
fn platform_math(section: &str, path: &str) -> bool {
    matches!(
        (section, path),
        ("village", "r.4.3" | "r.5.1" | "r.7.0.0.4") | ("quarry", "r.2.3")
    )
}

/// The line with its vertex and instance hashes blanked, keeping the counts.
fn without_float_hashes(line: &str) -> String {
    line.split('|')
        .map(|field| match field.split_once('=') {
            Some((key @ ("P" | "N" | "U" | "C"), _)) => format!("{key}=*"),
            Some(("INST", value)) => format!("INST={}", value.split(',').next().unwrap_or("")),
            _ => field.to_string(),
        })
        .collect::<Vec<_>>()
        .join("|")
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
        // Harbor Havoc's scenery and containers were redrawn after the port; the
        // harbor budget tests bound them instead.
        // Pine Village was repainted the same way (tree atlas, shared matte paint,
        // knolls beyond the wall); `forest_normals_match_typescript_on_a_grid`
        // and the showcase budget bound it.
        if name.starts_with("harbor")
            || name.starts_with("container-")
            || name == "village"
            || name == "waterwheel"
        {
            continue;
        }
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
            } else if platform_math(name, path) {
                (without_float_hashes(a), without_float_hashes(b))
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

/// Normals of the background forest batches on a 1/65536 grid
/// (`Math.round(n * 65536) >>> 0`, FNV-1a): how many batches the cells hold and
/// the FNV-1a of their hashes in order, as the Pine Village art pass left them
/// (one atlas batch per cell and compass sector; far conifers as solid whorls,
/// a mostly conifer near treeline).
const FOREST_NORMALS: (usize, u32) = (16, 0x7fef_0909);

#[test]
fn forest_normals_match_typescript_on_a_grid() {
    let village = VillageScenery::new();
    let forest = &village.root.children[4].children[5];
    // The forest is batched per compass sector: hash every sector's batches in turn.
    let hashes: Vec<u32> =
        forest
            .children
            .iter()
            .flat_map(|cell| &cell.children)
            .map(|batch| {
                let mesh = &batch.drawable.as_ref().expect("a batch").mesh;
                fnv(mesh.normals.iter().flatten().map(|&n| {
                    crate::geometry::math::js_round(f64::from(n) * 65536.0) as i64 as u32
                }))
            })
            .collect();
    let combined = (hashes.len(), fnv(hashes.iter().copied()));
    assert_eq!(combined, FOREST_NORMALS, "{combined:#010x?}");
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
            "textures/harbor/container.webp",
            "generated:harbor-container-liveries",
            "generated:harbor-labels",
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
        effects_scenery::HARBOR_LABEL_TEXTURE,
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
    // Cover containers stand in every view, and in the shadow pass.
    assert!(cover < 4_000, "cover {cover} triangles");
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
    // Supply is painted, not lit from within.
    assert!(face.material.map.is_some());
    assert!(face.material.emissive_map.is_none());
    let village = VillageScenery::new();
    let smoke = village.root.find(CHIMNEY_SMOKE_NODE).unwrap();
    let mesh = &smoke.drawable.as_ref().unwrap().mesh;
    assert!(mesh.attributes.iter().all(|a| a.per_instance));
    // The sand round a cover rock is its own skirt, shaded as the floor in the
    // rock's draw: the footing adds no blended draw of its own.
    let footing = sandstone_footing(14.0, 8.0, 37);
    assert!(footing.drawable.is_none() && footing.children.is_empty());
}

/// Printed by the reference script (see the module docs), compacted. The
/// `village` section is the Rust output of the Pine Village art pass that
/// re-authored its ground, boundary walls, backdrop and forest after the port.
#[rustfmt::skip]
const EXPECTED: &str = r#"
# village
r|pine-village-scenery|1|9|=
r.0|-|1|0|0,-0.8,0|0,0,0,1|1,1,1|V=324|I=-1|P=e793c4f9|N=04279eb5|U=29c6023d|C=c0134d31|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.1|-|1|0|0,0.008,0|0,0,0,1|1,1,1|V=2401|I=13824|P=da6a05a7|N=6b6c5f6d|U=39dcbecd|C=5651af37|A=-|X=-|M=d5f93363|F=0,1,0,1
r.2|-|1|1|=
r.2.0|spawn-pads|1|4|=
r.2.0.0|-|1|0|=|V=14400|I=-1|P=baa5d0e5|N=9de8fa65|U=a13b5035|C=008df045|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.2.0.1|-|1|0|=|V=4620|I=-1|P=d05dcf47|N=68402dbd|U=f9864bd5|C=69c9f195|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.2.0.2|-|1|0|=|V=32400|I=-1|P=06cd4685|N=8e0ad905|U=c4155b65|C=ce6e67c5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.2.0.3|-|1|0|=|V=4620|I=-1|P=9fa5f5e7|N=c84f2335|U=f9864bd5|C=4366ccc1|A=-|X=-|M=d33b7c4a|F=1,1,0,1
r.3|-|1|4|=
r.3.0|-|1|0|=|V=6912|I=-1|P=d1f60c39|N=99646dcd|U=c32c0d3d|C=-|A=-|X=-|M=9b6457e0|F=1,1,0,1
r.3.1|-|1|0|=|V=1440|I=-1|P=bea9573d|N=0f558125|U=2767ac45|C=-|A=-|X=-|M=5d24fa8b|F=1,1,0,1
r.3.2|-|1|0|=|V=1440|I=-1|P=6f7173bd|N=0f558125|U=e4e4a4f5|C=-|A=-|X=-|M=c7181185|F=1,1,0,1
r.3.3|-|1|0|=|V=840|I=-1|P=91f1b8f1|N=92726fa5|U=-|C=55e50889|A=-|X=-|M=c37f9530|F=1,1,0,1
r.4|pine-valley-landscape|1|6|=
r.4.0|-|1|0|=|V=12769|I=75264|P=e64f13b9|N=3f8186ec|U=365fd62d|C=c32bf8b3|A=-|X=-|M=d5f93363|F=0,1,0,1
r.4.1|village-creek|1|0|0,-2.65,0|-0.707106781,0,0,0.707106781|1,1,1|V=322|I=960|P=0884e83d|N=1b97729d|U=be71afd5|C=-|A=-|X=-|M=d4dd116a|F=0,0,0,1
r.4.2|pine-mountain-ridge|1|0|=|V=2457|I=14040|P=0a69319a|N=16026be3|U=4a0d61cb|C=31e7ad36|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.4.3|-|1|0|=|V=10201|I=38988|P=dbc4039c|N=10b45c69|U=-|C=5c9ddccc|A=-|X=-|M=e19d6c00|F=0,0,0,1
r.4.4|-|1|1|=
r.4.4.0|-|1|0|=|V=13200|I=-1|P=4adf01fe|N=7a449f8b|U=a43e5825|C=0bcca9fd|A=-|X=-|M=4083e32b|F=1,1,0,1
r.4.5|-|1|16|=
r.4.5.0|-|1|4|=
r.4.5.0.0|-|1|0|=|V=96|I=-1|P=dc006313|N=e124188d|U=986104c5|C=27c121f5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.0.1|-|1|0|=|V=1890|I=-1|P=d41f5371|N=7bc35227|U=a9702415|C=5160e1fe|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.0.2|-|1|0|=|V=384|I=-1|P=ef42b7b5|N=d7a493b5|U=a283da05|C=eecc4747|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.0.3|-|1|0|=|V=5328|I=-1|P=d91ecffb|N=a1736043|U=20cfd245|C=6f573c2b|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.1|-|1|4|=
r.4.5.1.0|-|1|0|=|V=144|I=-1|P=da24d39a|N=63a5d23b|U=e01a1845|C=b2289275|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.1.1|-|1|0|=|V=3780|I=-1|P=1326fe72|N=da2710c6|U=562aaf65|C=c27b0f74|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.1.2|-|1|0|=|V=606|I=-1|P=1671c10e|N=1d232cbf|U=9197edcd|C=fb5fa181|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.1.3|-|1|0|=|V=7326|I=-1|P=a30ee45f|N=18be6837|U=8cb76c75|C=eefa60cf|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.2|-|1|4|=
r.4.5.2.0|-|1|0|=|V=606|I=-1|P=48153ea6|N=a9f3e373|U=7d5f91cd|C=94c0ecc9|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.2.1|-|1|0|=|V=7326|I=-1|P=131526b7|N=d201c125|U=8cb76c75|C=9dbfae3b|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.2.2|-|1|0|=|V=3150|I=-1|P=6baf4038|N=23193f66|U=1812d3f5|C=3e085667|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.2.3|-|1|0|=|V=96|I=-1|P=612af37b|N=72cd5697|U=986104c5|C=cae957c5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.3|-|1|4|=
r.4.5.3.0|-|1|0|=|V=126|I=-1|P=c845fbbe|N=0b063df3|U=e9b6948d|C=ad15ff2d|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.3.1|-|1|0|=|V=1998|I=-1|P=8fc2a303|N=bcccb07c|U=be373ff5|C=1e9522f6|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.3.2|-|1|0|=|V=240|I=-1|P=be19ab02|N=00e350ed|U=fe59ff45|C=a0e06c45|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.3.3|-|1|0|=|V=3150|I=-1|P=09a67a1d|N=f51590fc|U=af92d3f5|C=482856c7|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.4|-|1|4|=
r.4.5.4.0|-|1|0|=|V=306|I=-1|P=29e3a260|N=8677c0c9|U=4fcd1d9d|C=6591fbfb|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.4.1|-|1|0|=|V=3330|I=-1|P=524cdc94|N=83f1e7cc|U=afd9bd15|C=9607a09e|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.4.2|-|1|0|=|V=240|I=-1|P=f0b9855a|N=8beac59d|U=fe59ff45|C=f775e435|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.4.3|-|1|0|=|V=4410|I=-1|P=64846172|N=09141e6a|U=856677d5|C=a507b953|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.5|-|1|4|=
r.4.5.5.0|-|1|0|=|V=42|I=-1|P=18c4b76d|N=d3daf121|U=e34db4bd|C=b5603657|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.5.1|-|1|0|=|V=666|I=-1|P=a50f6944|N=bd50ee08|U=f3d4f6d5|C=3a45e466|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.5.2|-|1|0|=|V=48|I=-1|P=315440fa|N=5cf5372f|U=5c973145|C=c80fe5c5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.5.3|-|1|0|=|V=630|I=-1|P=b77fd914|N=a65bee02|U=6952e835|C=813f86a8|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.6|-|1|3|=
r.4.5.6.0|-|1|0|=|V=132|I=-1|P=21c7422d|N=f16f6675|U=6e58a395|C=390c4161|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.6.1|-|1|0|=|V=630|I=-1|P=8b292e8e|N=33119db6|U=5c52e835|C=af4839a4|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.6.2|-|1|0|=|V=1332|I=-1|P=115cc149|N=9d3658c2|U=e053fce5|C=ee6ccfb0|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.7|-|1|4|=
r.4.5.7.0|-|1|0|=|V=360|I=-1|P=c7a323ab|N=31b4c0e5|U=18934c25|C=f8d689ed|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.7.1|-|1|0|=|V=2664|I=-1|P=d21367bb|N=78705a35|U=b6f15005|C=90c5e357|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.7.2|-|1|0|=|V=288|I=-1|P=5b1139bb|N=b6d67c07|U=5460d2c5|C=bf1c7b45|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.7.3|-|1|0|=|V=6300|I=-1|P=0072959d|N=1d10bc58|U=48c5bf25|C=7aabb195|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.8|-|1|4|=
r.4.5.8.0|-|1|0|=|V=2064|I=-1|P=6ceab288|N=3ca1dabf|U=8c1e2445|C=577d65c5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.8.1|-|1|0|=|V=34440|I=-1|P=b10642da|N=445a8817|U=0d71bd05|C=27aa7056|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.8.2|-|1|0|=|V=5568|I=-1|P=b3652505|N=667a95f1|U=e92a1185|C=e373cb8b|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.8.3|-|1|0|=|V=32736|I=-1|P=155256ad|N=7f847476|U=2da3d0c5|C=6e5fa235|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.9|-|1|4|=
r.4.5.9.0|-|1|0|=|V=4884|I=-1|P=ec7d381b|N=7e658cf3|U=6ca16595|C=a74f7337|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.9.1|-|1|0|=|V=30504|I=-1|P=bd851102|N=b67765fd|U=025c7e05|C=4421d876|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.9.2|-|1|0|=|V=25620|I=-1|P=c4c25525|N=18de9065|U=2076a7e5|C=1c197e1a|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.9.3|-|1|0|=|V=1488|I=-1|P=d390f75a|N=9d327137|U=dc87ba45|C=4160fce5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.10|-|1|4|=
r.4.5.10.0|-|1|0|=|V=5172|I=-1|P=a202912b|N=5645deeb|U=d118f0d5|C=de5cc67d|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.10.1|-|1|0|=|V=28980|I=-1|P=1ec62f2b|N=72d3a736|U=13e3dce5|C=a4777609|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.10.2|-|1|0|=|V=33480|I=-1|P=ea81e295|N=a2ac4ccd|U=c91b3705|C=0d31fdb6|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.10.3|-|1|0|=|V=1920|I=-1|P=0aec4d43|N=03c2a771|U=5afaa9c5|C=1191f2f5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.11|-|1|4|=
r.4.5.11.0|-|1|0|=|V=4188|I=-1|P=84d78221|N=8ea747bd|U=0c0eb0f5|C=76677bb7|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.11.1|-|1|0|=|V=20160|I=-1|P=25a8444d|N=f877373c|U=21ce1bc5|C=44adbf8f|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.11.2|-|1|0|=|V=26040|I=-1|P=e759b7df|N=e4511b9b|U=6470c885|C=92d40c79|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.11.3|-|1|0|=|V=1056|I=-1|P=e01bcec6|N=b44fd597|U=07280ac5|C=58267335|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.12|-|1|4|=
r.4.5.12.0|-|1|0|=|V=2574|I=-1|P=4230e652|N=1ee71393|U=3425004d|C=d8123d63|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.12.1|-|1|0|=|V=15996|I=-1|P=fbf22820|N=9f3b0123|U=62760225|C=564c604c|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.12.2|-|1|0|=|V=1296|I=-1|P=176fc1cf|N=24e6f009|U=1912ec45|C=e46f8575|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.12.3|-|1|0|=|V=18060|I=-1|P=da4f9f3a|N=edb8557e|U=4a5938a5|C=1b80a7c3|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.13|-|1|4|=
r.4.5.13.0|-|1|0|=|V=912|I=-1|P=57a82fbe|N=512998ef|U=0d455045|C=049673c5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.13.1|-|1|0|=|V=15960|I=-1|P=2611e03d|N=99686bb2|U=41e74985|C=d8def5f7|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.13.2|-|1|0|=|V=3012|I=-1|P=8dac6a85|N=c28b427b|U=feaebc15|C=5297a3b3|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.13.3|-|1|0|=|V=18600|I=-1|P=07e6e179|N=6ad5026c|U=03c39a05|C=51238f69|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.14|-|1|4|=
r.4.5.14.0|-|1|0|=|V=720|I=-1|P=48e6bf0a|N=cdef3a6b|U=672c8245|C=93e8db15|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.14.1|-|1|0|=|V=9660|I=-1|P=5f970700|N=1db09700|U=47fe3425|C=1e90d3d5|A=-|X=-|M=a9902efd|F=1,1,0,1
r.4.5.14.2|-|1|0|=|V=1308|I=-1|P=04b0543e|N=b6cbdca5|U=e77d4875|C=f27438f5|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.14.3|-|1|0|=|V=8184|I=-1|P=145d2186|N=de7d55b9|U=955e7285|C=fc748b48|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.15|-|1|4|=
r.4.5.15.0|-|1|0|=|V=834|I=-1|P=c5758f8f|N=cc98922d|U=db5d0d5d|C=4e7597e9|A=-|X=-|M=411c49e7|F=1,1,0,1
r.4.5.15.1|-|1|0|=|V=4836|I=-1|P=078cd229|N=40a06dcb|U=7f84bc65|C=e386cc03|A=-|X=-|M=af1f4640|F=1,1,0,1
r.4.5.15.2|-|1|0|=|V=240|I=-1|P=d01af8ad|N=3e305c71|U=fe59ff45|C=7cc557d5|A=-|X=-|M=c69807ea|F=1,1,0,1
r.4.5.15.3|-|1|0|=|V=4620|I=-1|P=d5c6f673|N=991b3fda|U=68e364a5|C=c6efdf97|A=-|X=-|M=a9902efd|F=1,1,0,1
r.5|village-meadow|1|2|=
r.5.0|-|1|0|=|V=27|I=-1|P=1d9e1424|N=433300f9|U=-|C=df26c380|A=-|X=windOrigin:2:919cb229|M=3ac9b326|F=0,1,0,1|INST=3842,19025ea1,4d940d32
r.5.1|-|1|0|=|V=45|I=-1|P=091be9fe|N=37a17a27|U=-|C=e321dee6|A=-|X=-|M=679821f5|F=0,1,0,1|INST=702,7aab7f82,e4e56795
r.6|village-chimney-smoke|1|0|=|V=4|I=6|P=463c62b5|N=9d3c62b5|U=d6e17165|C=-|A=-|X=phase:1:a6f6753d;smokeOrigin:3:20e0d2c5|M=f5df665b|F=0,0,0,0|INST=0,-,-
r.7|-|1|5|=
r.7.0|village-farmstead|1|3|=
r.7.0.0|-|1|7|=
r.7.0.0.0|-|1|28|115.5,-0.797129522,-9|0,0,0,1|1,1,1
r.7.0.0.0.0|-|1|0|0,-0.5,0|0,0,0,1|1,1,1|V=324|I=-1|P=ed61db35|N=04279eb5|U=d49b64ed|C=-|A=-|X=-|M=4bca9de7|F=1,1,0,1
r.7.0.0.0.1|-|1|0|0,2.6,0|0,0,0,1|1,1,1|V=24|I=36|P=5e0e42ed|N=79efde65|U=8cbeff85|C=-|A=-|X=-|M=b3909d6a|F=1,1,0,1
r.7.0.0.0.2|-|1|0|0,4.9,0|0,0,0,1|1,1,1|V=24|I=-1|P=9cccc945|N=7424e3d9|U=f5c9ff85|C=-|A=-|X=-|M=b3909d6a|F=1,1,0,1
r.7.0.0.0.3|-|1|0|0,4.6,0|0,0,0,1|1,1,1|V=12|I=-1|P=5e091dd9|N=af8a4545|U=341eebd5|C=-|A=-|X=-|M=847135e0|F=1,1,0,1
r.7.0.0.0.4|-|1|0|-4.5,2.6,-6.5|0,0,0,1|1,1,1|V=24|I=36|P=172e12cd|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.5|-|1|0|-4.5,2.6,6.5|0,0,0,1|1,1,1|V=24|I=36|P=172e12cd|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.6|-|1|0|-4.54,4.78,0|0,0,0,1|1,1,1|V=24|I=36|P=c4656355|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.7|-|1|0|4.5,2.6,-6.5|0,0,0,1|1,1,1|V=24|I=36|P=172e12cd|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.8|-|1|0|4.5,2.6,6.5|0,0,0,1|1,1,1|V=24|I=36|P=172e12cd|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.9|-|1|0|4.54,4.78,0|0,0,0,1|1,1,1|V=24|I=36|P=c4656355|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.10|-|1|0|-4.58,2.1,-1.6|0,0,0,1|1,1,1|V=24|I=36|P=85994d0d|N=79efde65|U=cc25ff85|C=-|A=-|X=-|M=4da92371|F=1,1,0,1
r.7.0.0.0.11|-|1|0|-4.66,2.1,-1.6|0.336186482,0,0,0.941795439|1,1,1|V=24|I=36|P=bc93d37d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.12|-|1|0|-4.66,2.1,-1.6|-0.336186482,0,0,0.941795439|1,1,1|V=24|I=36|P=bc93d37d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.13|-|1|0|-4.66,0.4,-1.6|0,0,0,1|1,1,1|V=24|I=36|P=49b6e68d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.14|-|1|0|-4.66,3.8,-1.6|0,0,0,1|1,1,1|V=24|I=36|P=49b6e68d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.15|-|1|0|-4.58,2.1,1.6|0,0,0,1|1,1,1|V=24|I=36|P=85994d0d|N=79efde65|U=cc25ff85|C=-|A=-|X=-|M=4da92371|F=1,1,0,1
r.7.0.0.0.16|-|1|0|-4.66,2.1,1.6|0.336186482,0,0,0.941795439|1,1,1|V=24|I=36|P=bc93d37d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.17|-|1|0|-4.66,2.1,1.6|-0.336186482,0,0,0.941795439|1,1,1|V=24|I=36|P=bc93d37d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.18|-|1|0|-4.66,0.4,1.6|0,0,0,1|1,1,1|V=24|I=36|P=49b6e68d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.19|-|1|0|-4.66,3.8,1.6|0,0,0,1|1,1,1|V=24|I=36|P=49b6e68d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.20|-|1|0|-4.6,4.05,0|0,0,0,1|1,1,1|V=24|I=36|P=0826c935|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=d10a0af7|F=1,1,0,1
r.7.0.0.0.21|-|1|0|0,5.4,-6.54|0,0,0,1|1,1,1|V=24|I=36|P=92d7861d|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=03cdaca3|F=1,1,0,1
r.7.0.0.0.22|-|1|0|0,5.4,-6.57|0,0,0,1|1,1,1|V=24|I=36|P=ec686455|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=64e37bcf|F=1,1,0,1
r.7.0.0.0.23|-|1|0|0,4.9,-6.58|0,0,0,1|1,1,1|V=24|I=36|P=ef8b3725|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=d1354b85|F=1,1,0,1
r.7.0.0.0.24|-|1|0|0,8.5,0|0,0,0,1|1,1,1|V=24|I=36|P=244ec1e5|N=79efde65|U=b4a5ff85|C=-|A=-|X=-|M=1d2db508|F=1,1,0,1
r.7.0.0.0.25|-|1|0|-0.56,8.55,0|0,0,0,1|1,1,1|V=24|I=36|P=0dfbe7bd|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=d10a0af7|F=1,1,0,1
r.7.0.0.0.26|-|1|0|0.56,8.55,0|0,0,0,1|1,1,1|V=24|I=36|P=0dfbe7bd|N=79efde65|U=9a55ff85|C=-|A=-|X=-|M=d10a0af7|F=1,1,0,1
r.7.0.0.0.27|-|1|0|0,8.98,0|0,0,0,1|1,1,1|V=12|I=-1|P=e6136c85|N=911df7b5|U=2cea14a5|C=-|A=-|X=-|M=847135e0|F=1,1,0,1
r.7.0.0.1|-|1|0|=|V=4512|I=-1|P=f73e36c7|N=a5208d84|U=7d1fd8f5|C=45693085|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.0.0.2|-|1|0|=|V=192|I=-1|P=2ab51e21|N=55022e8b|U=c3f33fb5|C=df7f2085|A=-|X=-|M=bb740227|F=1,1,0,1
r.7.0.0.3|-|1|0|=|V=1464|I=-1|P=da8cc68d|N=f0acf388|U=0d5849bd|C=30d98d65|A=-|X=-|M=e19d6c00|F=1,1,0,1
r.7.0.0.4|-|1|0|=|V=13608|I=-1|P=f5ec02e7|N=8b8f42f1|U=6da9ae05|C=b3f4547d|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.0.0.5|-|1|0|=|V=60|I=-1|P=66495fb7|N=2eb83d81|U=c7637825|C=f01e0399|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.0.0.6|-|1|0|=|V=12|I=-1|P=d393b851|N=6835e875|U=aad94985|C=869e4195|A=-|X=-|M=839d0a3e|F=1,1,0,1
r.7.0.1|-|1|2|97,0.21482137,-30|0,0,0,1|1,1,1
r.7.0.1.0|-|1|101|=
r.7.0.1.0.0|-|1|0|=|V=303|I=-1|P=9836327e|N=36c86bfd|U=9c89df7d|C=-|A=-|X=-|M=4c361339|F=1,1,0,1
r.7.0.1.0.1|-|1|0|=|V=8184|I=-1|P=47de83fc|N=e78c2d44|U=f4019e99|C=-|A=-|X=-|M=40dcd500|F=1,1,0,1
r.7.0.1.0.2|-|1|0|=|V=5208|I=-1|P=664ca513|N=3ec1286d|U=f2a4bd05|C=-|A=-|X=-|M=acc0154a|F=1,1,0,1
r.7.0.1.0.3|-|1|0|=|V=60|I=-1|P=de1bbb58|N=2bae4e85|U=1ff13995|C=-|A=-|X=-|M=778b90ae|F=1,1,0,1
r.7.0.1.0.4|-|1|0|=|V=948|I=-1|P=3e40a63f|N=7d8210a9|U=3e0ee61c|C=-|A=-|X=-|M=60b35ed6|F=1,1,0,1
r.7.0.1.0.5|-|1|0|=|V=1404|I=-1|P=8a518d91|N=9f14ee3d|U=f563b2de|C=-|A=-|X=-|M=63705ba2|F=1,1,0,1
r.7.0.1.0.6|-|1|0|=|V=1404|I=-1|P=c467cb1f|N=450125c7|U=2775beac|C=-|A=-|X=-|M=9b6457e0|F=1,1,0,1
r.7.0.1.0.7|-|1|0|=|V=348|I=-1|P=bd99c35f|N=b3a2ec27|U=5336d99c|C=-|A=-|X=-|M=5d24fa8b|F=1,1,0,1
r.7.0.1.0.8|-|1|0|=|V=120|I=-1|P=0813cec5|N=557f03e5|U=d5798949|C=-|A=-|X=-|M=29ac7bd2|F=1,1,0,1
r.7.0.1.0.9|-|1|0|=|V=252|I=-1|P=6fa0e2db|N=18eefcd5|U=c3de6575|C=-|A=-|X=-|M=6fe52461|F=1,1,0,1
r.7.0.1.0.10|-|1|0|=|V=252|I=-1|P=07ea4a8b|N=18eefcd5|U=8208bd6d|C=-|A=-|X=-|M=2337e1db|F=1,1,0,1
r.7.0.1.0.11|-|1|0|=|V=432|I=-1|P=4b2ae193|N=6ae76708|U=ecbfb840|C=-|A=-|X=-|M=b1a67b95|F=1,1,0,1
r.7.0.1.0.12|-|1|0|=|V=144|I=-1|P=60c45a70|N=84f811ed|U=72e0a553|C=-|A=-|X=-|M=50401627|F=1,1,0,1
r.7.0.1.0.13|-|1|0|=|V=948|I=-1|P=7a5241ab|N=1811daf5|U=0ffe75fd|C=-|A=-|X=-|M=c7181185|F=1,1,0,1
r.7.0.1.0.14|-|1|0|=|V=36|I=-1|P=8edb86e5|N=b47a9e35|U=c3f596d5|C=-|A=-|X=-|M=c545410c|F=1,1,0,1
r.7.0.1.0.15|-|1|0|=|V=108|I=-1|P=6a417905|N=65210275|U=b7e16db1|C=-|A=-|X=-|M=2058c161|F=1,1,0,1
r.7.0.1.0.16|-|1|0|=|V=1008|I=-1|P=109f2f9f|N=1c0c38b9|U=77316461|C=-|A=-|X=-|M=208595bc|F=1,1,0,1
r.7.0.1.0.17|-|1|0|-2.02277969,1.1728557,3.62845249|0,0.999978075,0,0.00662188759|0.0987158323,0.0851751471,0.0774418746|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.18|-|1|0|-1.93896782,1.17707022,3.69585006|0,0.517984241,0,0.85539016|0.086856179,0.0918037256,0.0778456738|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.19|-|1|0|-1.81457217,1.20427439,3.64141614|0,0.366174886,0,0.930546051|0.0869436927,0.111166519,0.0829405538|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.20|-|1|0|-1.73950759,1.1890393,3.65087153|0,0.191989469,0,0.981396986|0.0930608756,0.0722350356,0.0797058045|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.21|-|1|0|-1.59387418,1.1762304,3.66389971|0,0.753530166,0,0.657413332|0.0844216128,0.11524866,0.0768626058|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.22|-|1|0|-1.54102973,1.18636255,3.61076191|0,0.978800463,0,0.204816146|0.0810245236,0.109123936,0.0779725722|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.23|-|1|0|-1.42265335,1.18822475,3.70123687|0,0.922948822,0,0.384922682|0.0936516639,0.114486023,0.0990904074|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.24|-|1|0|-1.29496067,1.18895378,3.6837977|0,0.899772153,0,0.436360027|0.099165327,0.111554607,0.079119517|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.25|-|1|0|-1.18121939,1.17691175,3.67934253|0,0.822748015,0,0.568406284|0.0703797576,0.111880579,0.0774187221|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.26|-|1|0|-1.20534918,1.28754415,3.69410466|0,0,0,1|0.0445448459,0.0311813921,0.0445448459|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.27|-|1|0|-1.7188949,1.30204972,3.71063293|0,0,0,1|0.0346696063,0.0242687244,0.0346696063|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.28|-|1|0|-1.85062974,1.24912251,3.69748547|0,0,0,1|0.0323757354,0.0226630148,0.0323757354|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.29|-|1|0|-2.00084686,1.30108597,3.75733154|0,0,0,1|0.0362832861,0.0253983002,0.0362832861|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.30|-|1|0|-1.95575004,1.29861583,3.68713803|0,0,0,1|0.0435320041,0.0304724028,0.0435320041|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.31|-|1|0|-1.91782263,1.26006581,3.75520616|0,0,0,1|0.0434857715,0.0304400401,0.0434857715|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.32|-|1|0|-1.7012139,1.26773313,3.70206933|0,0,0,1|0.0379601082,0.0265720757,0.0379601082|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.33|-|1|0|-1.45907222,1.25835982,3.72142327|0,0,0,1|0.0414388854,0.0290072198,0.0414388854|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.34|-|1|0|-1.85394827,1.27444247,3.6206346|0,0,0,1|0.0316469783,0.0221528848,0.0316469783|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.35|-|1|0|-1.85135592,1.24423011,3.59235979|0,0,0,1|0.0417438865,0.0292207206,0.0417438865|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.36|-|1|0|-1.81243463,1.29980378,3.61873298|0,0,0,1|0.0316489557,0.022154269,0.0316489557|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.37|-|1|0|-1.29443237,1.25253617,3.6797436|0,0,0,1|0.0315038598,0.0220527019,0.0315038598|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.38|-|1|0|1.22693493,1.17200688,3.65286758|0,0.56300057,0,0.826456507|0.0750963951,0.103254955,0.0701994591|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.39|-|1|0|1.30933746,1.17520682,3.70186247|0,0.3375859,0,0.941294725|0.0827002139,0.0718039997,0.0837665396|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.40|-|1|0|1.41365018,1.17269056,3.64369829|0,0.82571169,0,0.564092373|0.0848538565,0.0978867357,0.0701233809|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.41|-|1|0|1.54083452,1.18824947,3.69404921|0,0.994454045,0,0.105172009|0.0715497677,0.11409402,0.0802091344|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.42|-|1|0|1.60469575,1.19470801,3.70737745|0,0.500167484,0,0.865928685|0.0710126329,0.0971387931,0.0876708519|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.43|-|1|0|1.70867451,1.18530793,3.63175619|0,0.478974862,0,0.877828617|0.0787054752,0.116488114,0.0774071647|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.44|-|1|0|1.83729926,1.19780557,3.62464783|0,0.997799624,0,0.0663016673|0.0702717499,0.0847634563,0.0833731127|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.45|-|1|0|1.95608932,1.18863983,3.71708019|0,0.874100416,0,0.485745265|0.0800172321,0.10963549,0.0984052307|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.46|-|1|0|2.05479274,1.18839241,3.68518182|0,0.0826345844,0,0.996579914|0.090447719,0.118075252,0.0817933681|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.47|-|1|0|1.44224878,1.27562522,3.67240706|0,0,0,1|0.04209551,0.029466857,0.04209551|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.48|-|1|0|1.20469544,1.30216649,3.7513363|0,0,0,1|0.0345923114,0.024214618,0.0345923114|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.49|-|1|0|2.03962312,1.29737379,3.65228438|0,0,0,1|0.0354980455,0.0248486318,0.0354980455|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.50|-|1|0|2.00019198,1.24356002,3.75020438|0,0,0,1|0.0304894869,0.0213426408,0.0304894869|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.51|-|1|0|1.17257664,1.27138159,3.65809834|0,0,0,1|0.0372364915,0.026065544,0.0372364915|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.52|-|1|0|1.8123672,1.25376481,3.59261492|0,0,0,1|0.0390112675,0.0273078872,0.0390112675|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.53|-|1|0|1.71426628,1.29447388,3.58488486|0,0,0,1|0.0425062843,0.029754399,0.0425062843|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.54|-|1|0|1.33214133,1.25591405,3.69224953|0,0,0,1|0.0389499159,0.0272649411,0.0389499159|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.55|-|1|0|2.0091084,1.27446882,3.68967916|0,0,0,1|0.0392437265,0.0274706086,0.0392437265|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.56|-|1|0|1.71141719,1.25542102,3.66441829|0,0,0,1|0.0435970104,0.0305179073,0.0435970104|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.57|-|1|0|1.8055622,1.27409852,3.62714218|0,0,0,1|0.0329790048,0.0230853034,0.0329790048|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.58|-|1|0|1.70952091,1.28693009,3.6437535|0,0,0,1|0.0374753378,0.0262327365,0.0374753378|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.59|-|1|0|2.04033752,1.19640196,-3.67746112|0,0.557865723,0,0.829931223|0.089330283,0.0716020348,0.08379238|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.60|-|1|0|1.92619066,1.18145646,-3.62476506|0,0.701286114,0,0.712879925|0.0752265779,0.0997411157,0.0766442872|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.61|-|1|0|1.79980956,1.18591774,-3.69894553|0,0.361899545,0,0.932217099|0.0906272174,0.112555463,0.0750017984|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.62|-|1|0|1.72390853,1.20600027,-3.6789562|0,0.771144379,0,0.636660307|0.0727676998,0.0908959497,0.0934089351|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.63|-|1|0|1.61616016,1.19370949,-3.71827654|0,0.867838942,0,0.49684562|0.0898147519,0.0744779294,0.0821264966|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.64|-|1|0|1.50998491,1.20393415,-3.62895778|0,0.699122355,0,0.715002051|0.0705179119,0.115905097,0.0854663198|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.65|-|1|0|1.3898507,1.18823742,-3.6482198|0,0.991454385,0,0.130453834|0.0904465564,0.105406107,0.0739454373|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.66|-|1|0|1.33187274,1.17488699,-3.64478309|0,0.512982135,0,0.858399283|0.0766466552,0.111349549,0.0974479682|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.67|-|1|0|1.20550682,1.17063078,-3.72501728|0,0.513773198,0,0.857926046|0.0807827406,0.102294966,0.0883819594|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.68|-|1|0|1.82147378,1.27614548,-3.71242331|0,0,0,1|0.0441715201,0.0309200641,0.0441715201|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.69|-|1|0|1.62886306,1.31248633,-3.59277449|0,0,0,1|0.0379914522,0.0265940165,0.0379914522|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.70|-|1|0|1.3935797,1.29259554,-3.72281967|0,0,0,1|0.0317712422,0.0222398695,0.0317712422|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.71|-|1|0|1.95253276,1.30242721,-3.75637942|0,0,0,1|0.0352892946,0.0247025062,0.0352892946|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.72|-|1|0|1.37129957,1.30099816,-3.71697578|0,0,0,1|0.0412387546,0.0288671282,0.0412387546|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.73|-|1|0|1.78856825,1.24264212,-3.60501129|0,0,0,1|0.0387593823,0.0271315676,0.0387593823|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.74|-|1|0|1.66667379,1.27959413,-3.71101349|0,0,0,1|0.0369899375,0.0258929563,0.0369899375|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.75|-|1|0|1.59585372,1.29004081,-3.67957634|0,0,0,1|0.0288876774,0.0202213742,0.0288876774|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.76|-|1|0|1.2102495,1.27712205,-3.71750151|0,0,0,1|0.0436483696,0.0305538587,0.0436483696|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.77|-|1|0|1.95480804,1.2595257,-3.68897876|0,0,0,1|0.0334739877,0.0234317914,0.0334739877|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.78|-|1|0|2.01253226,1.31097494,-3.6932025|0,0,0,1|0.0326865733,0.0228806013,0.0326865733|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.79|-|1|0|1.69723182,1.31661656,-3.74813441|0,0,0,1|0.0314149313,0.0219904519,0.0314149313|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.80|-|1|0|-1.22452049,1.18899205,-3.61547732|0,0.998209019,0,0.0598226892|0.0999296792,0.0740062171,0.0759124364|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.81|-|1|0|-1.30554702,1.18950117,-3.67608926|0,0.576156967,0,0.817339066|0.0861418499,0.0767378053,0.09291897|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.82|-|1|0|-1.42976425,1.20260935,-3.65674412|0,0.560988882,0,0.827823335|0.0996657543,0.100200518,0.0890505983|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.83|-|1|0|-1.50213445,1.18627256,-3.7148488|0,0.764086219,0,0.645114137|0.0720489454,0.117306675,0.0760764221|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.84|-|1|0|-1.62695053,1.18983346,-3.62005357|0,0.943991001,0,0.329971195|0.0842783515,0.0701215948,0.0875757413|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.85|-|1|0|-1.71299292,1.20230417,-3.700901|0,0.417531407,0,0.908662492|0.0795885489,0.0833108911,0.0840634859|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=cdb7a605|F=1,1,0,1
r.7.0.1.0.86|-|1|0|-1.83089435,1.19693086,-3.70082699|0,0.679665026,0,0.733522632|0.0793045418,0.0773449741,0.0830618283|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.87|-|1|0|-1.90811513,1.20732683,-3.70038462|0,0.728884338,0,0.684636854|0.0817649791,0.0875194365,0.0798960573|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.88|-|1|0|-2.0131567,1.17470942,-3.66940048|0,0.991133858,0,0.132867135|0.0903512684,0.0829032414,0.0748528632|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=5d456919|F=1,1,0,1
r.7.0.1.0.89|-|1|0|-1.50806291,1.31457086,-3.7024824|0,0,0,1|0.0290771604,0.0203540123,0.0290771604|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.90|-|1|0|-1.38889707,1.24447152,-3.62864555|0,0,0,1|0.0374428221,0.0262099755,0.0374428221|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.91|-|1|0|-1.94801848,1.31718561,-3.66655605|0,0,0,1|0.0397371575,0.0278160103,0.0397371575|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.92|-|1|0|-1.67191689,1.26006102,-3.68348709|0,0,0,1|0.0339415902,0.0237591132,0.0339415902|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.93|-|1|0|-1.5801718,1.31046697,-3.62958056|0,0,0,1|0.0331370755,0.0231959529,0.0331370755|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.94|-|1|0|-1.7938787,1.24337736,-3.68912936|0,0,0,1|0.0354095259,0.0247866681,0.0354095259|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.95|-|1|0|-1.526119,1.28144013,-3.59199305|0,0,0,1|0.0328435309,0.0229904716,0.0328435309|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.96|-|1|0|-1.32142819,1.30331798,-3.70839753|0,0,0,1|0.0401124775,0.0280787343,0.0401124775|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.97|-|1|0|-1.31260448,1.3048734,-3.60227614|0,0,0,1|0.0329142999,0.0230400099,0.0329142999|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.98|-|1|0|-1.76927794,1.31186818,-3.74253801|0,0,0,1|0.0396752943,0.027772706,0.0396752943|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.99|-|1|0|-1.77809259,1.28163617,-3.63639543|0,0,0,1|0.0428954299,0.0300268009,0.0428954299|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.0.100|-|1|0|-1.36820258,1.28972553,-3.61788171|0,0,0,1|0.0427862515,0.0299503761,0.0427862515|V=24|I=-1|P=8fefde65|N=ea4d8f85|U=3b55ff85|C=-|A=-|X=-|M=229b3501|F=1,1,0,1
r.7.0.1.1|-|1|0|0,-0.8,0|0,0,0,1|1,1,1|V=324|I=-1|P=8d39cd25|N=04279eb5|U=a3a3d5e5|C=-|A=-|X=-|M=4bca9de7|F=1,1,0,1
r.7.0.2|village-log-cart|1|3|102,-0.878537372,-8|0,-0.434965534,0,0.900447102|1,1,1
r.7.0.2.0|-|1|0|=|V=36|I=-1|P=a3137221|N=f87a9e35|U=c0efde65|C=c7066fc5|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.0.2.1|-|1|0|=|V=3492|I=-1|P=f8dcb593|N=15d07677|U=1efa79f9|C=f9352601|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.0.2.2|-|1|0|=|V=72|I=-1|P=9b32b929|N=f197b4d5|U=155c3305|C=4f70298d|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.1|pine-watermill|1|2|-35,-0.25,-69|0,0,0,1|1,1,1
r.7.1.0|-|1|5|=
r.7.1.0.0|-|1|0|=|V=2916|I=-1|P=bbfe227d|N=07df6f35|U=c45ce495|C=6cd04935|A=-|X=-|M=ec422f06|F=1,1,0,1
r.7.1.0.1|-|1|0|=|V=564|I=-1|P=0c12c179|N=f3c53715|U=bf0a44e5|C=688fcfcd|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.1.0.2|-|1|0|=|V=36|I=-1|P=c3f7eebf|N=e2bc5439|U=8bc9ae45|C=2fe06845|A=-|X=-|M=839d0a3e|F=1,1,0,1
r.7.1.0.3|-|1|0|=|V=540|I=-1|P=0cd2bc3b|N=e4e64755|U=2d081b25|C=18a110ed|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.1.0.4|-|1|0|=|V=120|I=-1|P=0b7b18fd|N=c07e01ab|U=3dd9bc15|C=8ec68e45|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.1.1|turning-waterwheel|1|4|-8,1.1,-13.6|0,0,0,1|1,1,1
r.7.1.1.0|-|1|0|=|V=3456|I=-1|P=4df199d5|N=004a3eb5|U=e4e1b7a5|C=8554adc5|A=-|X=-|M=dc999f91|F=1,1,0,1
r.7.1.1.1|-|1|0|=|V=864|I=-1|P=3002ceed|N=0285bb85|U=9f583cc5|C=612550a5|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.1.1.2|-|1|0|=|V=720|I=-1|P=d8d61cd5|N=e2cda325|U=12948245|C=00040625|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.1.1.3|-|1|0|=|V=144|I=-1|P=67f7205f|N=a782a24b|U=6d704989|C=865e1a55|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.2|village-timber-bridge|1|3|0,0,-82|0,0,0,1|1,1,1
r.7.2.0|-|1|0|=|V=1116|I=-1|P=5eb9e741|N=c9a95785|U=a6284525|C=4d9ba871|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.2.1|-|1|0|=|V=2304|I=-1|P=069c8835|N=c16a2445|U=964a45c5|C=1360caed|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.2.2|-|1|0|=|V=648|I=-1|P=fff41961|N=f1cbd625|U=80872625|C=ce775aa5|A=-|X=-|M=ec422f06|F=1,1,0,1
r.7.3|village-timber-bridge|1|3|-85,0,38|0,0.707106781,0,0.707106781|1,1,1
r.7.3.0|-|1|0|=|V=1116|I=-1|P=5eb9e741|N=c9a95785|U=a6284525|C=4d9ba871|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.3.1|-|1|0|=|V=2304|I=-1|P=069c8835|N=c16a2445|U=964a45c5|C=1360caed|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.7.3.2|-|1|0|=|V=648|I=-1|P=fff41961|N=f1cbd625|U=80872625|C=ce775aa5|A=-|X=-|M=ec422f06|F=1,1,0,1
r.7.4|village-log-cart|1|3|-68,-0.65,-24|0,0.0848976828,0,0.996389675|1,1,1
r.7.4.0|-|1|0|=|V=36|I=-1|P=a3137221|N=f87a9e35|U=c0efde65|C=c7066fc5|A=-|X=-|M=98e0a408|F=1,1,0,1
r.7.4.1|-|1|0|=|V=3492|I=-1|P=f8dcb593|N=15d07677|U=1efa79f9|C=f9352601|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.7.4.2|-|1|0|=|V=72|I=-1|P=9b32b929|N=f197b4d5|U=155c3305|C=4f70298d|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.8|-|1|2|=
r.8.0|-|1|0|=|V=2052|I=-1|P=d28a69cb|N=3b68aea1|U=df7ddf85|C=4c16bbe1|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.8.1|-|1|0|=|V=36|I=-1|P=552c6071|N=f87a9e35|U=5aefde65|C=e27a9e35|A=-|X=-|M=05369b81|F=1,1,0,1
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
r|dusty-dig-scenery|1|5|=
r.0|quarry-compacted-haul-roads|1|0|0,0.008,0|0,0,0,1|1,1,1|V=1681|I=9600|P=5595aedf|N=c837573c|U=3191f5a5|C=6078ecf7|A=-|X=-|M=e19d6c00|F=0,1,0,1
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
r.2.8|-|1|0|=|V=5400|I=-1|P=9cb95e51|N=4ecab20b|U=3d87bfa5|C=6d29d530|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3|-|1|180|=
r.3.0|-|1|8|0,-1.8,-77|0,1,0,0|1,1,1
r.3.0.0|-|1|0|=|V=2808|I=-1|P=d7733fdc|N=b6fdc244|U=d2441e9a|C=2a096a4a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.1|-|1|0|=|V=2808|I=-1|P=b620231c|N=a6deef96|U=519806af|C=b7560546|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.2|-|1|0|=|V=2808|I=-1|P=a813b5c8|N=f74cc2a6|U=daa4346b|C=78401553|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.3|-|1|0|=|V=2808|I=-1|P=bc7cbdca|N=4afb2602|U=7cbbb1d5|C=1755a6c4|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.4|-|1|0|=|V=2808|I=-1|P=4ac8c7f1|N=118368e8|U=4dc54da4|C=117f279e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.5|-|1|0|=|V=2808|I=-1|P=80c35402|N=4fc7f1c4|U=42a34fca|C=83f3267e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.6|-|1|0|=|V=2808|I=-1|P=1631a4d7|N=a1150c1f|U=6d5d9c08|C=bf99b6f7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.0.7|-|1|0|=|V=2808|I=-1|P=3a93b23d|N=6ef7a72c|U=559bb1e4|C=19660ba7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1|-|1|8|0,2.5,-95|0,1,0,0|1,1,1
r.3.1.0|-|1|0|=|V=3432|I=-1|P=c89ebe69|N=8197dada|U=b0e522f4|C=dfd18f74|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.1|-|1|0|=|V=3432|I=-1|P=81e9327c|N=cd5d45a4|U=30d559ae|C=0191c9bb|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.2|-|1|0|=|V=3432|I=-1|P=31993a94|N=387acd0d|U=8abcdd19|C=316c3c91|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.3|-|1|0|=|V=3432|I=-1|P=8e7985ca|N=be60d3af|U=f55710b4|C=8a352469|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.4|-|1|0|=|V=3432|I=-1|P=7d887ddc|N=c29156c7|U=ab51ca56|C=5741605f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.5|-|1|0|=|V=3432|I=-1|P=d50e16cb|N=52e4f398|U=bc5eac5a|C=e74794a6|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.6|-|1|0|=|V=3432|I=-1|P=068b9fd9|N=d936fd52|U=f0a2bd1f|C=5da60020|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.1.7|-|1|0|=|V=3432|I=-1|P=0640ef01|N=18ffeb40|U=5789df91|C=e77e0f0e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2|-|1|8|0,7.8,-115|0,1,0,0|1,1,1
r.3.2.0|-|1|0|=|V=4056|I=-1|P=3021e903|N=83dc58d0|U=fdfacd5e|C=ab6f4b5b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.1|-|1|0|=|V=4056|I=-1|P=1a8ccbe1|N=44881296|U=7b0c75fd|C=e32d4f00|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.2|-|1|0|=|V=4056|I=-1|P=5ac2124b|N=da202306|U=5b2c9231|C=f918c990|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.3|-|1|0|=|V=4056|I=-1|P=96776614|N=d62f1c13|U=bc7252ca|C=4a288089|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.4|-|1|0|=|V=4056|I=-1|P=fe119755|N=12e644bd|U=bffc5110|C=a2e59d23|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.5|-|1|0|=|V=4056|I=-1|P=172e79a9|N=4b61ab37|U=7d032e34|C=2ec9ef96|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.6|-|1|0|=|V=4056|I=-1|P=d7d3ea75|N=557dd88c|U=7cec4549|C=d3e2f06f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.2.7|-|1|0|=|V=4056|I=-1|P=dd021f43|N=fefbe964|U=25b1dbaa|C=4dfc818c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3|-|1|5|-80,-1.8,0|0,-0.707106781,0,0.707106781|1,1,1
r.3.3.0|-|1|0|=|V=2184|I=-1|P=898083ed|N=625be8b9|U=b16fe995|C=d2e6a274|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.1|-|1|0|=|V=2184|I=-1|P=5aae26ed|N=6e70bb53|U=c321448b|C=2bdafbe6|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.2|-|1|0|=|V=2184|I=-1|P=8ee8c29b|N=18294a7d|U=21b962cf|C=9575688f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.3|-|1|0|=|V=2184|I=-1|P=6d2c3b47|N=4112df44|U=42bc2e21|C=36cdacb5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.3.4|-|1|0|=|V=924|I=-1|P=96e8e59e|N=d9baf2fa|U=7bfc2f3a|C=406c944a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4|-|1|5|-96,2.2,0|0,-0.707106781,0,0.707106781|1,1,1
r.3.4.0|-|1|0|=|V=2808|I=-1|P=f2a9640f|N=cfbb78ac|U=7be3853a|C=81f1398d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.1|-|1|0|=|V=2808|I=-1|P=c2fd06ea|N=bf20cbeb|U=e3b41b32|C=0acb9286|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.2|-|1|0|=|V=2808|I=-1|P=983ab220|N=2e8843c6|U=e5f9e5ff|C=74f15409|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.3|-|1|0|=|V=2808|I=-1|P=065f4fb6|N=13f21c6f|U=5e34edef|C=1272c3b8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.4.4|-|1|0|=|V=1188|I=-1|P=2dc32c43|N=2d64c54b|U=4558c298|C=64d7e96a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5|-|1|8|0,-1.8,78|0,0,0,1|1,1,1
r.3.5.0|-|1|0|=|V=2184|I=-1|P=fe101ff2|N=3d2997d6|U=82807829|C=bd5d139b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.1|-|1|0|=|V=2184|I=-1|P=a30125fd|N=fb0b5a45|U=d57b46fc|C=b9d0a9ce|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.2|-|1|0|=|V=2184|I=-1|P=94352320|N=7092f0d5|U=5e8e1ced|C=17ee23c6|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.3|-|1|0|=|V=2184|I=-1|P=9035a68e|N=5c2375ce|U=27adb619|C=87619475|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.4|-|1|0|=|V=2184|I=-1|P=194e95ad|N=89307be7|U=11ef2435|C=e847a9f2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.5|-|1|0|=|V=2184|I=-1|P=1b98bd29|N=718be9f4|U=2811363d|C=881642d3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.6|-|1|0|=|V=2184|I=-1|P=cde89fdd|N=521247b6|U=91f8adc8|C=ecd9c3ad|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.5.7|-|1|0|=|V=2184|I=-1|P=032983ca|N=4a65f683|U=37144831|C=7f9047c0|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6|-|1|8|0,2.5,93|0,0,0,1|1,1,1
r.3.6.0|-|1|0|=|V=3432|I=-1|P=3be0fb0d|N=fbb06f3f|U=12d5726a|C=aab5d0b9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.1|-|1|0|=|V=3432|I=-1|P=9fee30d1|N=36537529|U=778ada7e|C=2fb21664|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.2|-|1|0|=|V=3432|I=-1|P=9222d358|N=448e98b2|U=19ddcb5f|C=4e19b1eb|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.3|-|1|0|=|V=3432|I=-1|P=333d4f0e|N=d1a11560|U=3201ba05|C=612ba74e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.4|-|1|0|=|V=3432|I=-1|P=b2933ac7|N=21dceeef|U=23632ac0|C=957133c3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.5|-|1|0|=|V=3432|I=-1|P=a11632ad|N=cffde6b6|U=51dee69a|C=b01b8f4a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.6|-|1|0|=|V=3432|I=-1|P=87daf573|N=17bd0a8c|U=f5f3d72f|C=35c6ed05|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.6.7|-|1|0|=|V=3432|I=-1|P=06398812|N=e0922013|U=7876d955|C=a34611e6|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7|-|1|8|0,7.8,114|0,0,0,1|1,1,1
r.3.7.0|-|1|0|=|V=4056|I=-1|P=af8a0365|N=7e1ec095|U=0e3b56d5|C=374d8a4c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.1|-|1|0|=|V=4056|I=-1|P=947f5493|N=e872cdf9|U=4b98a44e|C=6bd89474|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.2|-|1|0|=|V=4056|I=-1|P=29232567|N=30c21681|U=4d4d1b1e|C=99b49bce|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.3|-|1|0|=|V=4056|I=-1|P=6c413df7|N=c8957f64|U=c92bb3ee|C=4c8c9521|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.4|-|1|0|=|V=4056|I=-1|P=47cfa8ed|N=82e56f6f|U=200064e3|C=f3bc37b7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.5|-|1|0|=|V=4056|I=-1|P=4ab764a1|N=39949f67|U=9d1bd4c2|C=57cba848|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.6|-|1|0|=|V=4056|I=-1|P=43d62fe1|N=307ae847|U=2201098d|C=fe4bad87|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.7.7|-|1|0|=|V=4056|I=-1|P=95f4f24a|N=cb187e22|U=0712dece|C=2ed79383|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8|-|1|5|78,-1.8,0|0,0.707106781,0,0.707106781|1,1,1
r.3.8.0|-|1|0|=|V=2808|I=-1|P=8381e6ab|N=e86736bf|U=c3e59a81|C=b7dddfe7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.1|-|1|0|=|V=2808|I=-1|P=38add22e|N=ed5d73e7|U=4d96ebd0|C=23c531a3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.2|-|1|0|=|V=2808|I=-1|P=f59efe02|N=9d15c74e|U=ee24fccf|C=f31336ad|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.3|-|1|0|=|V=2808|I=-1|P=aea2f96d|N=e10bb91f|U=36d4c974|C=429c0bf7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.8.4|-|1|0|=|V=1188|I=-1|P=3ef5414a|N=e782ee91|U=1574efe1|C=2eb2ae13|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9|-|1|5|97,2.2,0|0,0.707106781,0,0.707106781|1,1,1
r.3.9.0|-|1|0|=|V=3432|I=-1|P=1cab8709|N=b50700cd|U=ac69200f|C=7a5a1690|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.1|-|1|0|=|V=3432|I=-1|P=3aa071c9|N=6fb02a04|U=68368ead|C=12e77d90|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.2|-|1|0|=|V=3432|I=-1|P=b7963f41|N=e85cabf1|U=11aab140|C=3e21eaa1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.3|-|1|0|=|V=3432|I=-1|P=2bfd56a7|N=0b7fbee3|U=55de1ca8|C=d03208d9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.9.4|-|1|0|=|V=1452|I=-1|P=f26a9196|N=731cfb52|U=a00097ab|C=5543fe4d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.10|-|1|0|=|V=1029|I=5256|P=6770e728|N=e4c986ee|U=da31e729|C=7e87937b|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.11|-|1|0|=|V=2820|I=-1|P=feed9309|N=0bab2c36|U=bcb9e315|C=e5981105|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.12|-|1|0|=|V=3408|I=-1|P=f00b4d12|N=fe4e2b19|U=6d934805|C=8c5002fe|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.13|-|1|0|=|V=3348|I=-1|P=c951ec7b|N=d4b0f9a3|U=98a84c55|C=9da2e057|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.14|-|1|0|=|V=3852|I=-1|P=0fd9bbf2|N=0188d91c|U=b574e6b5|C=95f1aa8b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.15|-|1|0|=|V=1029|I=5256|P=649bc15c|N=16a271ba|U=187b71ca|C=e46ae406|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.16|-|1|0|=|V=2904|I=-1|P=f9f68570|N=13756c65|U=fce2cca5|C=f40ef0b7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.17|-|1|0|=|V=3216|I=-1|P=d2f4ef9c|N=a72e99a2|U=a77b9905|C=5c2e4678|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.18|-|1|0|=|V=2532|I=-1|P=84973bbb|N=2caff2db|U=70db3c95|C=b688896c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.19|-|1|0|=|V=3840|I=-1|P=d90095c2|N=d8a9acdc|U=a6edc9c5|C=172f81ed|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.20|-|1|0|=|V=994|I=5076|P=1b9969ed|N=e0205c00|U=e3951690|C=b719b383|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.21|-|1|0|=|V=2796|I=-1|P=7e98189d|N=e00e4d5f|U=870cfe35|C=0a2d5772|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.22|-|1|0|=|V=3276|I=-1|P=253da536|N=305132ab|U=23f7f1b5|C=da528a90|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.23|-|1|0|=|V=3168|I=-1|P=de97f079|N=6ceab2f5|U=fa4f6545|C=937e7ea1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.24|-|1|0|=|V=2796|I=-1|P=85f76fa8|N=7f0804ba|U=2164fe35|C=dc6ee528|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.25|-|1|0|=|V=637|I=3240|P=12a2176d|N=61970bd3|U=af86b8c8|C=767156a5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.26|-|1|0|=|V=3720|I=-1|P=44120b0b|N=0e1c811c|U=b6d49c65|C=7653b280|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.27|-|1|0|=|V=3288|I=-1|P=dd020050|N=a196ae74|U=38befaa5|C=3d97bc7f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.28|-|1|0|=|V=1440|I=-1|P=6e63a6d0|N=a0cb59c9|U=d8351e45|C=71607e67|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.29|-|1|0|=|V=325|I=1836|P=9412e326|N=688befb2|U=67c7a736|C=a4b3eeb7|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.30|-|1|0|=|V=2400|I=-1|P=d78889b7|N=b6100488|U=fccc0945|C=1bd24913|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.31|-|1|0|=|V=325|I=1836|P=a12460ef|N=f5d23e98|U=572e6ffa|C=a8fce57a|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.32|-|1|0|=|V=1752|I=-1|P=a4993f39|N=c57fc3ae|U=09db62a5|C=36aec186|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.33|-|1|0|=|V=325|I=1836|P=0a15d405|N=488300e1|U=25656476|C=2a90379d|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.34|-|1|0|=|V=1572|I=-1|P=7268b578|N=f62ed879|U=a040f195|C=ac9f6a77|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.35|-|1|0|=|V=325|I=1836|P=709478f7|N=c3183c80|U=857227a8|C=60365059|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.36|-|1|0|=|V=312|I=576|P=7caacc7d|N=b25a9bf1|U=e56bb8fd|C=b65a7425|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.37|-|1|0|53.4418097,-1.47769216,65.3015381|0,-0.248457977,0,0.968642676|1,1,1|V=432|I=-1|P=f859eb73|N=e689f298|U=8e540905|C=888a7fc7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.38|-|1|0|11.5612569,-0.954262195,-64.1383604|0,0.117717649,0,0.993047106|1,1,1|V=432|I=-1|P=af6c1616|N=3681f475|U=d090b485|C=925266f3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.39|-|1|0|-2.69994512,-1.792,66.8020833|0,0.253605816,0,0.96730765|1,1,1|V=432|I=-1|P=13aed8d7|N=7f72f270|U=462a0105|C=79167915|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.40|-|1|0|-48.9208838,-1.792,-67.0518412|0,0.0595308913,0,0.998226464|1,1,1|V=432|I=-1|P=33021beb|N=b1822702|U=adaf3185|C=3ad5404d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.41|-|1|0|28.1156319,-1.29775483,64.9016774|0,-0.277700753,0,0.960667628|1,1,1|V=432|I=-1|P=a3fc8a76|N=9e3201db|U=8e540905|C=3e650dab|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.42|-|1|0|-46.6000823,-1.14344502,-64.5587667|0,-0.313796598,0,0.949490229|1,1,1|V=432|I=-1|P=1fe2b037|N=d8979034|U=d090b485|C=f80c463c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.43|-|1|0|44.4080773,-1.792,67.9264632|0,-0.445843696,0,0.89511083|1,1,1|V=432|I=-1|P=6fdf8561|N=6f9d912e|U=462a0105|C=0960c59f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.44|-|1|0|-29.3710009,-1.49848239,-65.3477386|0,-0.31791694,0,0.948118568|1,1,1|V=432|I=-1|P=3b19c03e|N=c0b7d3d1|U=adaf3185|C=135357df|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.45|-|1|0|-23.7655446,-1.792,66.7772296|0,-0.451022152,0,0.892512755|1,1,1|V=432|I=-1|P=9d857e8a|N=5a524f66|U=8e540905|C=e9bddcb9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.46|-|1|0|-60.245454,-1.792,-66.0064346|0,-0.301808867,0,0.953368453|1,1,1|V=432|I=-1|P=24a9164b|N=0e8eacc4|U=d090b485|C=0619e0d1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.47|-|1|0|-3.67290316,-1.792,69.8801957|0,-0.431522192,0,0.902102321|1,1,1|V=432|I=-1|P=b52a0fce|N=fcc249ea|U=462a0105|C=fd4876ef|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.48|-|1|0|25.0121961,-1.792,-67.5945274|0,-0.16328635,0,0.986578719|1,1,1|V=432|I=-1|P=d6d8f513|N=72a00725|U=adaf3185|C=129e7947|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.49|-|1|0|53.2066484,-1.792,67.0761624|0,0.0260808147,0,0.999659838|1,1,1|V=432|I=-1|P=f859eb73|N=e689f298|U=8e540905|C=888a7fc7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.50|-|1|0|-36.0437085,-1.792,-66.3119011|0,-0.244566075,0,0.969632629|1,1,1|V=432|I=-1|P=af6c1616|N=3681f475|U=d090b485|C=925266f3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.51|-|1|0|-19.4125404,-1.36848599,65.0588578|0,0.419225892,0,0.907881959|1,1,1|V=432|I=-1|P=13aed8d7|N=7f72f270|U=462a0105|C=79167915|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.52|-|1|0|-43.7779759,-1.59067533,-65.5526119|0,0.313892929,0,0.949458387|1,1,1|V=432|I=-1|P=33021beb|N=b1822702|U=adaf3185|C=3ad5404d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.53|-|1|0|12.118863,-1.792,67.545617|0,-0.252229166,0,0.96766753|1,1,1|V=432|I=-1|P=a3fc8a76|N=9e3201db|U=8e540905|C=3e650dab|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.54|-|1|0|-60.8312014,-1.04870996,-64.3482444|0,0.384651972,0,0.923061677|1,1,1|V=432|I=-1|P=1fe2b037|N=d8979034|U=d090b485|C=f80c463c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.55|-|1|0|56.8982346,-0.965907968,64.1642399|0,-0.410349976,0,0.911928121|1,1,1|V=432|I=-1|P=6fdf8561|N=6f9d912e|U=462a0105|C=0960c59f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.56|-|1|0|15.6800286,-1.60891152,-65.5931367|0,-0.134866838,0,0.990863732|1,1,1|V=432|I=-1|P=3b19c03e|N=c0b7d3d1|U=adaf3185|C=135357df|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.57|-|1|0|-23.4142764,-1.792,67.9819035|0,-0.335279092,0,0.942118851|1,1,1|V=432|I=-1|P=9d857e8a|N=5a524f66|U=8e540905|C=e9bddcb9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.58|-|1|0|24.9816007,-1.792,-68.2192064|0,-0.420813299,0,0.907147269|1,1,1|V=432|I=-1|P=24a9164b|N=0e8eacc4|U=d090b485|C=0619e0d1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.59|-|1|0|9.47656552,-1.792,68.8020492|0,-0.258646743,0,0.965971978|1,1,1|V=432|I=-1|P=b52a0fce|N=fcc249ea|U=462a0105|C=fd4876ef|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.60|-|1|0|32.099017,-1.792,-66.6731991|0,0.152393049,0,0.988319968|1,1,1|V=432|I=-1|P=d6d8f513|N=72a00725|U=adaf3185|C=129e7947|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.61|-|1|0|-1.87008293,-1.792,69.9976535|0,-0.208030137,0,0.978122417|1,1,1|V=432|I=-1|P=f859eb73|N=e689f298|U=8e540905|C=888a7fc7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.62|-|1|0|-55.783674,-1.792,-68.9809425|0,-0.206180006,0,0.97851408|1,1,1|V=432|I=-1|P=af6c1616|N=3681f475|U=d090b485|C=925266f3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.63|-|1|0|-51.1684044,-1.792,67.5878489|0,0.291019412,0,0.956717148|1,1,1|V=432|I=-1|P=13aed8d7|N=7f72f270|U=462a0105|C=79167915|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.64|-|1|0|-31.1858579,-1.792,-69.7955642|0,0.399078982,0,0.916916553|1,1,1|V=432|I=-1|P=33021beb|N=b1822702|U=adaf3185|C=3ad5404d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.65|-|1|0|-47.193834,-1.792,67.667591|0,0.398820626,0,0.917028957|1,1,1|V=432|I=-1|P=a3fc8a76|N=9e3201db|U=8e540905|C=3e650dab|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.66|-|1|0|16.0283491,-1.792,-69.1583298|0,-0.478228128,0,0.87823565|1,1,1|V=432|I=-1|P=1fe2b037|N=d8979034|U=d090b485|C=f80c463c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.67|-|1|0|38.966499,-1.15245461,64.578788|0,-0.138374204,0,0.990380018|1,1,1|V=432|I=-1|P=6fdf8561|N=6f9d912e|U=462a0105|C=0960c59f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.68|-|1|0|34.4395304,-1.792,-69.3199753|0,-0.302129061,0,0.95326703|1,1,1|V=432|I=-1|P=3b19c03e|N=c0b7d3d1|U=adaf3185|C=135357df|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.69|-|1|0|39.6885582,-1.792,66.7125111|0,0.109516229,0,0.993985008|1,1,1|V=432|I=-1|P=9d857e8a|N=5a524f66|U=8e540905|C=e9bddcb9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.70|-|1|0|21.5423346,-1.39433403,-65.1162978|0,0.388156381,0,0.921593524|1,1,1|V=432|I=-1|P=24a9164b|N=0e8eacc4|U=d090b485|C=0619e0d1|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.71|-|1|0|-14.5380362,-1.03769861,64.3237747|0,-0.289171289,0,0.957277371|1,1,1|V=432|I=-1|P=b52a0fce|N=fcc249ea|U=462a0105|C=fd4876ef|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.72|-|1|0|-9.77980259,-1.792,-66.4151254|0,0.100185938,0,0.994968732|1,1,1|V=432|I=-1|P=d6d8f513|N=72a00725|U=adaf3185|C=129e7947|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.73|-|1|0|37.5160893,-1.792,66.4286164|0,-0.166270097,0,0.986080248|1,1,1|V=432|I=-1|P=f859eb73|N=e689f298|U=8e540905|C=888a7fc7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.74|-|1|0|27.0629233,-1.08340287,-64.4253397|0,-0.423598494,0,0.905850051|1,1,1|V=432|I=-1|P=af6c1616|N=3681f475|U=d090b485|C=925266f3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.75|-|1|0|-25.1396514,-1.792,69.1605661|0,0.325424413,0,0.945568058|1,1,1|V=432|I=-1|P=13aed8d7|N=7f72f270|U=462a0105|C=79167915|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.76|-|1|0|36.2431744,-1.792,-68.4139779|0,-0.205693342,0,0.978616497|1,1,1|V=432|I=-1|P=33021beb|N=b1822702|U=adaf3185|C=3ad5404d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.77|-|1|0|67.5435062,-1.932,-29.8622538|0,-0.441795081,0,0.897115994|1,1,1|V=432|I=-1|P=faa05651|N=51ac84ed|U=8e540905|C=061fb7e9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.78|-|1|0|65.3733601,-1.65001206,6.68824688|0,-0.594856883,0,0.80383163|1,1,1|V=432|I=-1|P=0f40d64a|N=6bd3b62a|U=462a0105|C=a73b8a5b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.79|-|1|0|-67.6598833,-1.932,47.7880139|0,-0.992077419,0,0.125628|1,1,1|V=432|I=-1|P=d5fd41bf|N=21881e43|U=adaf3185|C=c1f24903|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.80|-|1|0|66.3236551,-1.932,-0.400768523|0,-0.492058546,0,0.87056211|1,1,1|V=432|I=-1|P=7357f768|N=bad545d9|U=db879e05|C=7db4bc87|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.81|-|1|0|-68.992453,-1.932,-48.4416089|0,-0.392415374,0,0.919788114|1,1,1|V=432|I=-1|P=2fb51d25|N=c053a7f2|U=462a0105|C=6c8d03b9|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.82|-|1|0|64.7178277,-1.35502248,-41.5129149|0,0.966844442,0,0.25536606|1,1,1|V=432|I=-1|P=c151a455|N=be7d5298|U=adaf3185|C=e07f740b|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.83|-|1|0|-64.5053863,-1.25942384,-29.6631832|0,0.904703767,0,0.426041189|1,1,1|V=432|I=-1|P=9d6a3884|N=341a04e0|U=db879e05|C=13140ce2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.84|-|1|0|-66.5670903,-1.932,-26.6574722|0,0.753219041,0,0.657769775|1,1,1|V=432|I=-1|P=545b4e19|N=a16f5501|U=d090b485|C=455f7f43|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.85|-|1|0|-70.4058365,-1.932,-19.9776851|0,-0.738824017,0,0.673898414|1,1,1|V=432|I=-1|P=f8a761bb|N=c6b3942a|U=adaf3185|C=cd3dc903|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.86|-|1|0|-68.7370428,-1.932,-8.83575224|0,0.234092502,0,0.972214328|1,1,1|V=432|I=-1|P=cb19376b|N=8e9ca638|U=8e540905|C=e3a0c119|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.87|-|1|0|67.6551596,-1.932,3.82818438|0,-0.180086461,0,0.983650785|1,1,1|V=432|I=-1|P=1ac89cec|N=c19cee15|U=d090b485|C=8e076897|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.88|-|1|0|-69.1907948,-1.932,-34.9546219|0,-0.104002023,0,0.994577086|1,1,1|V=432|I=-1|P=508ec87e|N=c169c996|U=462a0105|C=8741f8bd|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.89|-|1|0|67.7026624,-1.932,7.84011993|0,0.943926446,0,0.330155818|1,1,1|V=432|I=-1|P=faf612aa|N=518b15fe|U=adaf3185|C=20d05389|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.90|-|1|0|-64.7443187,-1.36694344,-19.3156048|0,0.568256312,0,0.822851605|1,1,1|V=432|I=-1|P=20b2f221|N=647c8492|U=d090b485|C=91d19e59|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.91|-|1|0|65.2443975,-1.59197886,-42.5056273|0,-0.743337045,0,0.668917063|1,1,1|V=432|I=-1|P=80ef142e|N=aba16472|U=462a0105|C=2abaaa5c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.92|-|1|0|-65.7463804,-1.81787117,-32.0122369|0,-0.579772966,0,0.814778073|1,1,1|V=432|I=-1|P=1cf164c2|N=ed1f5c54|U=adaf3185|C=fc40fa46|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.93|-|1|0|66.8902038,-1.932,-37.0360571|0,-0.531215986,0,0.847236434|1,1,1|V=432|I=-1|P=3f269d92|N=6f2c3dca|U=d090b485|C=b3d04772|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.94|-|1|0|-65.776657,-1.83149565,18.9018475|0,0.766520585,0,0.642219739|1,1,1|V=432|I=-1|P=10952def|N=97b76436|U=462a0105|C=e37eba99|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.95|-|1|0|-65.9882417,-1.92670876,-30.4619959|0,-0.963347392,0,0.26825697|1,1,1|V=432|I=-1|P=2e2fed6c|N=66e72dcc|U=d090b485|C=d7c916d3|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.96|-|1|0|66.1581411,-1.932,-40.0591042|0,0.976689917,0,0.214655084|1,1,1|V=432|I=-1|P=c883cda8|N=e0869c73|U=462a0105|C=e1beff01|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.97|-|1|0|-70.3238675,-1.932,-26.9562622|0,0.99325525,0,0.115948301|1,1,1|V=432|I=-1|P=9622c2bd|N=5ae58982|U=adaf3185|C=40eae55d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.98|-|1|0|64.6619205,-1.32986424,-18.56406|0,0.963514931,0,0.267654586|1,1,1|V=432|I=-1|P=dfe81d61|N=0b46f876|U=db879e05|C=ceb7f0d5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.99|-|1|0|=|V=175|I=816|P=75e74885|N=59e18c86|U=5a9056c7|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.100|-|1|0|=|V=175|I=816|P=4a61534a|N=d2025466|U=fc1ec8ba|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.101|-|1|0|=|V=175|I=816|P=8c36bd48|N=1fccc281|U=95189c4d|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.102|-|1|0|=|V=175|I=816|P=89b83b89|N=e6fb9976|U=224182e1|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.103|-|1|0|=|V=30|I=120|P=3952edd7|N=4fec64be|U=a77620d0|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.104|-|1|0|=|V=175|I=816|P=073af0b7|N=28d60610|U=c8ea3b07|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.105|-|1|0|=|V=175|I=816|P=e47bbbc2|N=b1181eab|U=bef52566|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.106|-|1|0|=|V=175|I=816|P=fb6bae8a|N=0db1db39|U=ea51e56d|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.107|-|1|0|=|V=175|I=816|P=fb9b73e8|N=531a72e0|U=8e967809|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.108|-|1|0|=|V=30|I=120|P=135c1616|N=3d110b93|U=758d549c|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.109|-|1|0|=|V=175|I=816|P=11ce6e29|N=3981d08e|U=1358857b|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.110|-|1|0|=|V=175|I=816|P=801eae72|N=b013be26|U=dca7ab9e|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.111|-|1|0|=|V=175|I=816|P=a6e06a68|N=6c626df1|U=2d233ca0|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.112|-|1|0|=|V=175|I=816|P=c05b473d|N=895f4326|U=c586bceb|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.113|-|1|0|=|V=30|I=120|P=cd38ea03|N=b925c386|U=99737e1a|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.114|-|1|0|=|V=175|I=816|P=17a39a73|N=7586aac8|U=ce15593c|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.115|-|1|0|=|V=175|I=816|P=67c661aa|N=e51e3357|U=ea0c2b61|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.116|-|1|0|=|V=175|I=816|P=feac9cd6|N=23d0da95|U=7281757b|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.117|-|1|0|=|V=175|I=816|P=056e42cc|N=7980ea14|U=0f1c5190|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.118|-|1|0|=|V=30|I=120|P=87e9e186|N=0eed8aab|U=932404de|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.119|-|1|0|=|V=175|I=816|P=d9985ead|N=dd730657|U=b14d9151|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.120|-|1|0|=|V=175|I=816|P=c3f0527c|N=897b7e20|U=495e5f54|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.121|-|1|0|=|V=175|I=816|P=9ceb8fbd|N=a338dc4f|U=dc7f71f7|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.122|-|1|0|=|V=175|I=816|P=31f3c3bd|N=aa022c05|U=476472ab|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.123|-|1|0|=|V=30|I=120|P=c3914706|N=3a15facb|U=ed265ea8|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.124|-|1|0|=|V=175|I=816|P=d71a99ba|N=f40d6066|U=9b06cd38|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.125|-|1|0|=|V=175|I=816|P=11fa804d|N=045aeed3|U=dec01481|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.126|-|1|0|=|V=175|I=816|P=9f29c641|N=14c827c7|U=7c8e16f2|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.127|-|1|0|=|V=175|I=816|P=1dfbe26b|N=49dcdc92|U=2f7c4c7e|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.128|-|1|0|=|V=30|I=120|P=5620570e|N=39dd8bad|U=cc571cfc|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.129|-|1|0|=|V=175|I=816|P=d5098f95|N=a8bd6967|U=7339bc09|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.130|-|1|0|=|V=175|I=816|P=c0cb7628|N=eb1ec908|U=966f095c|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.131|-|1|0|=|V=175|I=816|P=dbbddaf1|N=10ce7d57|U=fc9d9e26|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.132|-|1|0|=|V=175|I=816|P=61a56d8d|N=c02cbd21|U=3ee9bf11|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.133|-|1|0|=|V=30|I=120|P=fda43026|N=5af2b7e3|U=132df832|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.134|-|1|0|=|V=175|I=816|P=43222ad2|N=202a02e2|U=20f2e941|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.135|-|1|0|=|V=175|I=816|P=a21f51f9|N=94da6afb|U=45d47eb8|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.136|-|1|0|=|V=175|I=816|P=c531e979|N=90fbd577|U=299a1c8a|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.137|-|1|0|=|V=175|I=816|P=b20b9f37|N=a309506e|U=bd6a75d9|C=41c685eb|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.138|-|1|0|=|V=30|I=120|P=e040bcbe|N=b8f2e66d|U=955facae|C=513f21e5|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.139|-|1|0|=|V=1296|I=5184|P=e11352f1|N=c039f62d|U=15601776|C=08bee6c5|A=05826bd5|X=-|M=7f8ad734|F=0,1,0,1
r.3.140|-|1|0|=|V=533|I=3186|P=34b79559|N=52027523|U=c05d883b|C=6911e147|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.141|-|1|0|=|V=9336|I=-1|P=e855e4a7|N=e6d0841f|U=6d121f25|C=581b4eb2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.142|-|1|0|=|V=413|I=2466|P=48fac337|N=4694c2df|U=150525c0|C=1bb480e7|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.143|-|1|0|=|V=6564|I=-1|P=b247b829|N=6e7476a7|U=bf403795|C=42ff5ba8|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.144|-|1|0|=|V=488|I=2916|P=882b4941|N=36226728|U=6f6247a4|C=55f82925|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.145|-|1|0|=|V=7788|I=-1|P=caef9b43|N=860924b1|U=67965435|C=534a4c3f|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.146|-|1|0|=|V=383|I=2286|P=0ef4bae6|N=1e662f13|U=e03b90ab|C=cc7e372f|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.147|-|1|0|=|V=5832|I=-1|P=819ff384|N=484c9c2f|U=84d35965|C=438f9f2e|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.148|-|1|0|=|V=458|I=2736|P=289f0299|N=cb7792ed|U=0381c086|C=3637f07d|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.149|-|1|0|=|V=8724|I=-1|P=f4163c46|N=762a5e25|U=0f571055|C=e1547436|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.150|-|1|0|=|V=413|I=2466|P=fb31876d|N=02b147f2|U=5f7f4250|C=1bb480e7|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.151|-|1|0|=|V=7176|I=-1|P=c6d974c1|N=c33802f6|U=bd939265|C=92166ff7|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.152|-|1|0|=|V=2976|I=17202|P=9bb95512|N=facad331|U=8556774d|C=6af784d4|A=-|X=-|M=e19d6c00|F=0,1,0,1
r.3.153|-|1|0|72.3167002,2.22919328,34.5|0,0.739082476,0,0.673614945|1,1,1|V=432|I=-1|P=b7f21cf0|N=029c13d4|U=8e540905|C=25720dd2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.154|-|1|0|72.4061484,2.17606008,38.8065454|0,0.463050536,0,0.88633188|1,1,1|V=432|I=-1|P=6b572b89|N=90b8634a|U=d090b485|C=f48540db|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.155|-|1|0|72.1896816,2.02398207,42.2155039|0,0.0927448153,0,0.995689911|1,1,1|V=432|I=-1|P=ae7387cf|N=0e8f98ca|U=462a0105|C=79e56781|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.156|-|1|0|72.0933678,1.5923361,45.4193106|0,0.833882494,0,0.551942013|1,1,1|V=432|I=-1|P=d3655c63|N=ab14178a|U=adaf3185|C=d7da76d5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.157|-|1|0|72.3520699,0.665658129,50.4926086|0,-0.845551883,0,0.53389326|1,1,1|V=432|I=-1|P=d871b2bb|N=ad7714eb|U=db879e05|C=64dc6f2a|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.158|-|1|0|72.3205345,-0.106701004,54.496324|0,-0.898517242,0,0.438938226|1,1,1|V=432|I=-1|P=581d1b1e|N=1d8a2118|U=8e540905|C=808f903c|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.159|-|1|0|72.3069881,-0.793234174,58.3899276|0,-0.884405889,0,0.466718571|1,1,1|V=432|I=-1|P=5b3ede04|N=96452dc7|U=d090b485|C=212dda91|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.160|-|1|0|74.7,2.25,32.4|0,0.33808304,0,0.941116283|1,1,1|V=432|I=-1|P=ebf04533|N=d2c6a36b|U=462a0105|C=08911403|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.161|-|1|0|77.8700953,2.25,32.4|0,0.0118779539,0,0.999929455|1,1,1|V=432|I=-1|P=2c45dc11|N=e3c1e1b0|U=adaf3185|C=a9b4b961|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.162|-|1|0|=|V=4128|I=-1|P=1cf404bc|N=43d002aa|U=a0a85045|C=7af47751|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.163|quarry-sentinel-butte|1|1|16,-1.8,-68|0,0,0,1|1,1,1
r.3.163.0|-|1|0|=|V=16380|I=-1|P=43686284|N=038b941e|U=9365e8ab|C=21b105b2|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.164|-|1|0|=|V=30498|I=-1|P=28271e22|N=bf163dad|U=9b898af5|C=b4122265|A=-|X=-|M=25f445b9|F=0,1,0,1
r.3.165|-|1|0|-51.2,-1.8,66.9|0,0.0199986667,0,0.999800007|1,1,1|V=120|I=168|P=72d65687|N=6d5d4826|U=f33f50f5|C=ccd1aced|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.166|-|1|0|-48.4,-1.8,66.8|0,-0.0149994375,0,0.999887502|1,1,1|V=120|I=168|P=7284b7b3|N=84d9be30|U=786750f5|C=1569542d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.167|-|1|0|-45.5,-1.8,67|0,0.0299955002,0,0.999550034|1,1,1|V=120|I=168|P=e22ee177|N=d8e9e7c3|U=ef4550f5|C=7dc4989d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.168|-|1|0|-50.3,-1.8,68.8|0,-0.0249973959,0,0.999687516|1,1,1|V=120|I=168|P=6d40885d|N=01dc808a|U=96c350f5|C=58f30b95|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.169|-|1|0|-47.2,-1.8,68.9|0,0.00999983333,0,0.99995|1,1,1|V=120|I=168|P=ab7fd4e1|N=7b9d963c|U=03a9c028|C=0625decd|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.170|-|1|0|-49.8,-0.68,67|0,0.0499791693,0,0.99875026|1,1,1|V=120|I=168|P=2ffc6f83|N=03c82c35|U=bd4350f5|C=5bab4105|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.171|-|1|0|-46.9,-0.62,67.1|0,-0.0399893342,0,0.999200107|1,1,1|V=120|I=168|P=f83e7545|N=e40eacae|U=f11a0805|C=b540b065|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.172|-|1|0|-48.6,-0.72,68.85|0,0.0249973959,0,0.999687516|1,1,1|V=120|I=168|P=aa53980b|N=278e52d0|U=e9a80805|C=fa83989d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.173|-|1|0|-42.6,-1.8,69.4|0,0.271546937,0,0.962425198|1,1,1|V=120|I=168|P=2299c117|N=68c1efc5|U=828350f5|C=2e0d69bd|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.174|-|1|0|-63.4,-0.9255,-41|0.0191132955,0.173055845,0.108101606,0.978774948|1,1,1|V=120|I=168|P=724830c9|N=bc58bd1f|U=089e79b5|C=cac86ddd|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.175|-|1|0|-63,-0.765,-14.5|0.0369715856,-0.244625879,-0.144792463,0.95803258|1,1,1|V=120|I=168|P=5454b29b|N=862a810e|U=171d50f5|C=41302e1d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.176|-|1|0|-63.2,-0.8235,51.5|0.0507492463,0.562357215,0.0741799677,0.821995261|1,1,1|V=120|I=168|P=70fccab3|N=86d87429|U=038350f5|C=14e68ef5|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.177|-|1|0|63.3,-0.895,-45|-0.0193722842,-0.148177158,0.128178491,0.980427722|1,1,1|V=120|I=168|P=b6061099|N=a5b63761|U=b74750f5|C=f436f85d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.178|-|1|0|63.1,-0.76,-31.5|-0.0434240954,0.432792518,-0.0898947107,0.895948617|1,1,1|V=120|I=168|P=e291928f|N=7a61245b|U=1ef0dc10|C=fa13f20d|A=-|X=-|M=25f445b9|F=1,1,0,1
r.3.179|-|1|0|63.4,-0.9525,47|0.0441619878,0.292201833,0.142763701,0.94460909|1,1,1|V=120|I=168|P=fd810239|N=8d8d68a1|U=10060805|C=469e3555|A=-|X=-|M=25f445b9|F=1,1,0,1
r.4|-|1|9|=
r.4.0|-|1|0|=|V=3240|I=-1|P=7a6adc2a|N=257e928d|U=105b3a05|C=cd459a65|A=-|X=-|M=5f0fd04f|F=1,1,0,1
r.4.1|-|1|0|=|V=1296|I=-1|P=2487532d|N=90d1d305|U=09332c25|C=48050b85|A=-|X=-|M=ec422f06|F=1,1,0,1
r.4.2|-|1|0|=|V=11496|I=-1|P=6791d68d|N=5b681245|U=91a520b1|C=9ec693e7|A=-|X=-|M=c3d0892e|F=1,1,0,1
r.4.3|-|1|0|=|V=11376|I=-1|P=56e0f063|N=e8c2582d|U=c3ee5e6d|C=f4ba10ee|A=-|X=-|M=90c71f94|F=1,1,0,1
r.4.4|-|1|0|=|V=540|I=-1|P=962cb757|N=58b93515|U=2d081b25|C=fd664a55|A=-|X=-|M=bf17ee3c|F=1,1,0,1
r.4.5|-|1|0|=|V=540|I=-1|P=3651a1a3|N=58b93515|U=2d081b25|C=01ebf371|A=-|X=-|M=d33b7c4a|F=1,1,0,1
r.4.6|-|1|0|=|V=3000|I=-1|P=85e8620b|N=9eb815af|U=-|C=c595af73|A=-|X=-|M=3ac9b326|F=1,1,0,1
r.4.7|-|1|0|=|V=6|I=-1|P=e2918304|N=9011a50d|U=8b3c62b5|C=ad11a50d|A=-|X=-|M=eb64e896|F=1,1,0,1
r.4.8|-|1|0|=|V=252|I=-1|P=3632694d|N=45b9c63d|U=06798625|C=0dbb2211|A=-|X=-|M=e325459e|F=1,1,0,1
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
r|-|1|0|=
# soil
accum|ee755f57
band-1000-1003|49c4908f
band-0-2|dff1a572
full|a19a1ff9
"#;
