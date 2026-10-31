//! The geometry cache's form (`scene_codec`) reads back every model exactly: the same
//! `model_pins` hash, the same sharing of meshes and materials within a model (the
//! renderer batches by `Arc` identity), and, in a fresh process that reads before it
//! builds, the same sharing with every model built afterwards (`shared_values`).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use glam::DMat4;
use sloppy_core::geometry::Mesh;
use sloppy_core::models::{
    CoverShape, MapTheme, Scenery, TreeDetail, WreckPart, batch, build_scenery, built_tank_models,
    cover_damage_stage, cover_model, offer_tank_models, tank_dimensions, tank_model_variant,
    wreck_model,
};
use sloppy_core::scene::{Material, Node};
use sloppy_core::scene_codec::{DecodeError, decode_bytes, encode};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{Team, VehicleKind};
use sloppy_core::sim::{RenderState, Simulation};

use crate::model_pin_hash::node_pin;

/// Every drawable's mesh and material identity, numbered in first-seen order: equal
/// for two trees exactly when they share alike.
fn sharing(root: &Node) -> Vec<(usize, usize)> {
    let mut meshes = HashMap::new();
    let mut materials = HashMap::new();
    let mut out = Vec::new();
    root.traverse(DMat4::IDENTITY, &mut |node, _| {
        if let Some(drawable) = &node.drawable {
            let next = meshes.len();
            let mesh = *meshes
                .entry(Arc::as_ptr(&drawable.mesh) as usize)
                .or_insert(next);
            let next = materials.len();
            let material = *materials
                .entry(Arc::as_ptr(&drawable.material) as usize)
                .or_insert(next);
            out.push((mesh, material));
        }
    });
    out
}

fn round_trip(what: &str, root: &Node) {
    let bytes = encode(root, "test/key").to_bytes();
    let decoded = decode_bytes(&bytes, "test/key").expect("reads back");
    assert_eq!(node_pin(&decoded), node_pin(root), "{what}: values");
    assert_eq!(sharing(&decoded), sharing(root), "{what}: sharing");
    assert_eq!(
        decode_bytes(&bytes, "other/key").err(),
        Some(DecodeError("built by another version")),
        "{what}"
    );
    assert!(decode_bytes(&bytes[..bytes.len() - 4], "test/key").is_err());
}

#[test]
fn scenery_reads_back_exactly() {
    for theme in [MapTheme::Village, MapTheme::Harbor, MapTheme::Quarry] {
        round_trip(&format!("{theme:?}"), build_scenery(theme).root());
    }
}

#[test]
fn vehicles_read_back_exactly() {
    for kind in VehicleKind::ALL {
        for team in [Team::Blue, Team::Red] {
            for open_turret_ring in [false, true] {
                tank_model_variant(kind, team, open_turret_ring);
            }
        }
    }
    let vehicles = built_tank_models().expect("every vehicle built");
    round_trip("vehicles", &vehicles);
}

const CACHE_DIR: &str = "SLOPPY_SCENE_CODEC_DIR";
const MODE: &str = "SLOPPY_SCENE_CODEC_MODE";

/// The counts of distinct meshes and materials a page's models hold: the harbor's
/// scenery and covers at every damage stage, every vehicle and every wreck, built in
/// the page's order (hulls measured first). With `read`, the vehicles and scenery come
/// from the files a `write` run left, as a returning visit's do.
fn census(dir: &Path, read: bool) -> (usize, usize) {
    let mut kept: Vec<(Arc<Mesh>, Arc<Material>)> = Vec::new();
    let mut add = |node: &Node| {
        node.traverse(DMat4::IDENTITY, &mut |node, _| {
            if let Some(drawable) = &node.drawable {
                kept.push((drawable.mesh.clone(), drawable.material.clone()));
            }
        });
    };
    if read {
        let bytes = std::fs::read(dir.join("vehicles")).expect("vehicles file");
        let group = decode_bytes(&bytes, "test/vehicles").expect("vehicles read back");
        assert!(offer_tank_models(group));
    }
    for kind in VehicleKind::ALL {
        tank_dimensions(kind);
    }
    let scenery = if read {
        let bytes = std::fs::read(dir.join("harbor")).expect("harbor file");
        let root = decode_bytes(&bytes, "test/harbor").expect("harbor read back");
        Scenery::from_root(MapTheme::Harbor, root)
    } else {
        build_scenery(MapTheme::Harbor)
    };
    add(scenery.root());
    let mut simulation = Simulation::with_seed(417.0);
    simulation.map_mode = MapId::Harbor;
    simulation.reset(None);
    let mut view = RenderState::default();
    simulation.fill_render_state(&mut view, None);
    for cover in &view.covers {
        let shape = CoverShape::from(cover);
        for fraction in [1.0, 0.6, 0.3, 0.1] {
            let stage = cover_damage_stage(cover.kind, cover.max_hp * fraction, cover.max_hp);
            let mut root = cover_model(&shape, TreeDetail::Full, stage);
            batch(&mut root);
            add(&root);
        }
    }
    for kind in VehicleKind::ALL {
        for team in [Team::Blue, Team::Red] {
            for open_turret_ring in [false, true] {
                add(&tank_model_variant(kind, team, open_turret_ring));
            }
            for part in [
                WreckPart::Intact,
                WreckPart::Hull,
                WreckPart::Turret,
                WreckPart::TurretBarrel,
                WreckPart::Barrel,
            ] {
                add(&wreck_model(kind, team, part));
            }
        }
    }
    if !read {
        let vehicles = built_tank_models().expect("every vehicle built");
        let files = [
            ("vehicles", encode(&vehicles, "test/vehicles").to_bytes()),
            ("harbor", encode(scenery.root(), "test/harbor").to_bytes()),
        ];
        for (name, bytes) in files {
            std::fs::write(dir.join(name), bytes).expect("a cache file");
        }
    }
    let meshes: HashSet<usize> = kept.iter().map(|(m, _)| Arc::as_ptr(m) as usize).collect();
    let materials: HashSet<usize> = kept.iter().map(|(_, m)| Arc::as_ptr(m) as usize).collect();
    (meshes.len(), materials.len())
}

/// One run of [`census`] in a process of its own, which [`fresh_processes_share_alike`]
/// starts: its stores begin empty, as a page's do.
#[test]
#[ignore = "run by fresh_processes_share_alike in a process of its own"]
fn census_in_this_process() {
    let dir = std::env::var(CACHE_DIR).expect("started by fresh_processes_share_alike");
    let read = std::env::var(MODE).is_ok_and(|mode| mode == "read");
    let (meshes, materials) = census(Path::new(&dir), read);
    println!("census {meshes} {materials}");
}

#[test]
fn fresh_processes_share_alike() {
    let dir = std::env::temp_dir().join(format!("sloppy-scene-codec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let run = |mode: &str| {
        let output = std::process::Command::new(std::env::current_exe().expect("this test"))
            .args([
                "--exact",
                "scene_codec::census_in_this_process",
                "--ignored",
                "--nocapture",
            ])
            .env(CACHE_DIR, &dir)
            .env(MODE, mode)
            .output()
            .expect("a child test process");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{mode}: {stdout}");
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("census "))
            .map(str::to_owned)
            .expect("the census line")
    };
    let built = run("write");
    let read = run("read");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(read, built, "meshes and materials, read back against built");
}
