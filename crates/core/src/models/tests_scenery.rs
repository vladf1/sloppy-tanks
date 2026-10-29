//! Scenery properties: the quarry soil bake's band split, the textures each theme
//! loads, triangle budgets, and what the renderer relies on from supply crates,
//! chimney smoke and rock footings. Scenery looks are judged on screen, not here.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use glam::DMat4;

use super::harbor_models::{CargoShape, shipping_container};
use super::quarry_surfaces::sandstone_footing;
use super::village_atmosphere::{WISPS_PER_SOURCE, set_chimney_smoke, village_atmosphere};
use super::*;
use crate::scene::{Node, TextureSource};

/// Built once for the tests that only read it.
static HARBOR: LazyLock<HarborScenery> = LazyLock::new(HarborScenery::new);

/// The page bakes the soil in row bands (workers, or slices between frames), and
/// each band first replays the seeded draws of the rows before it, so any split
/// gives the pixels of one whole bake.
#[test]
fn soil_is_independent_of_the_band_split() {
    let accum = sand_accum();
    let rows = 5;
    let whole = bake_quarry_soil(accum, 0, rows);
    let mut split = bake_quarry_soil(accum, 0, 2);
    split.extend(bake_quarry_soil(accum, 2, rows));
    assert!(
        whole == split,
        "a band baked alone differs from the whole bake"
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
    println!("quarry soil bake: {:.1} ms", bake.as_secs_f64() * 1e3);
    let start = Instant::now();
    let ground = bake_quarry_ground(sand_accum(), 0, QUARRY_SOIL_SIZE);
    println!(
        "quarry soil bake with its relief: {:.1} ms",
        start.elapsed().as_secs_f64() * 1e3
    );
    assert_eq!(ground.len(), soil.len());
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
    let names = |root: &Node| -> BTreeSet<String> {
        node_textures(root)
            .into_iter()
            .map(|source| match source {
                TextureSource::File(path) => path.to_string(),
                TextureSource::Generated(key) => format!("generated:{key}"),
            })
            .collect()
    };
    let set = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
    assert_eq!(
        names(&HARBOR.root),
        set(&[
            "textures/water/normals.webp",
            "textures/harbor/steel.webp",
            "textures/harbor/dock.webp",
            "textures/harbor/container-ribbed.webp",
            "generated:harbor-container-liveries",
            "generated:harbor-labels",
            "textures/harbor/container.webp",
            "generated:harbor-apron-wear",
        ])
    );
    assert_eq!(
        names(build_scenery(MapTheme::Quarry).root()),
        set(&[
            "generated:quarry-soil",
            "textures/ground/packed-dirt.webp",
            "textures/tanks/armor-wear.webp",
            "textures/tanks/armor-camo.webp",
            "textures/quarry/sandstone.webp",
            "textures/walls/weathered-concrete.webp",
            "textures/harbor/steel.webp",
            "generated:quarry-sign",
        ])
    );
    // The village also draws the tree models' textures.
    let village = names(build_scenery(MapTheme::Village).root());
    for texture in [
        "textures/ground/dry-grass.webp",
        "textures/ground/packed-dirt.webp",
        "textures/water/normals.webp",
        "textures/houses/siding.webp",
        "textures/houses/shingles.webp",
        "textures/walls/weathered-concrete.webp",
        "generated:village-sign",
    ] {
        assert!(village.contains(texture), "{texture}");
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
    for ship in &HARBOR.root.children[1].children[..3] {
        assert!(triangles(ship) < 25_000, "{} triangles", triangles(ship));
    }
}

#[test]
fn quarry_spawn_pads_stay_ankle_high() {
    let pieces = quarry_spawn_pad_pieces(Team::Red);
    assert!(!pieces.is_empty());
    assert!(pieces.iter().all(|p| p.y + p.h / 2.0 < 1.2));
}

/// Supply is military kit, painted rather than lit from within.
#[test]
fn supply_is_painted_not_glowing() {
    let ammo = pickup_cube(crate::sim::types::PickupKind::Rocket);
    let face = ammo.children[0].drawable.as_ref().unwrap();
    assert!(face.material.map.is_some());
    assert!(face.material.emissive_map.is_none());
}

/// Chimney wisps are per-instance attributes on one quad: the mill's chimney and
/// each indestructible house's smoke, eight wisps apiece, and nothing else does.
#[test]
fn only_the_mill_and_indestructible_houses_smoke() {
    let mut smoke = village_atmosphere();
    let mesh = &smoke.drawable.as_ref().unwrap().mesh;
    assert!(mesh.attributes.iter().all(|a| a.per_instance));
    let house = |destructible, kind| SmokeCover {
        kind,
        destructible,
        x: 10.0,
        z: 20.0,
        w: 8.0,
        h: 5.0,
        d: 6.0,
    };
    set_chimney_smoke(
        &mut smoke,
        &[
            house(false, "house"),
            house(true, "house"),
            house(false, "tree"),
        ],
    );
    let wisps = smoke.drawable.as_ref().unwrap().instances.as_ref().unwrap();
    assert_eq!(wisps.len(), 2 * WISPS_PER_SOURCE);
}

/// The sand round a cover rock is its own skirt, shaded as the floor in the rock's
/// draw: the footing adds no blended draw of its own.
#[test]
fn rock_footing_adds_no_draw() {
    let footing = sandstone_footing(14.0, 8.0, 37);
    assert!(footing.drawable.is_none() && footing.children.is_empty());
}
