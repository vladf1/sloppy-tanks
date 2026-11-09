//! Experiment: the coded reference house as an OBJ underlay for Blender, with its design,
//! roof and structure parts' boxes as JSON. `cargo run --example home_dump -- <out dir>`;
//! with a `.glb` as the second argument, also the imported model's triangles per part.

use sloppy_core::models::imported_house::{coded_pieces_obj, install, parse_glb};
use sloppy_core::models::structural_house;
use sloppy_core::scene::Node;
use sloppy_core::sim::structure::PartSet;

/// Draws and triangles of a model tree.
fn count(node: &Node) -> (usize, usize) {
    let mut totals = node.drawable.as_ref().map_or((0, 0), |drawable| {
        let mesh = &drawable.mesh;
        let triangles = mesh.indices.as_ref().map_or(mesh.positions.len(), Vec::len) / 3;
        let instances = drawable.instances.as_ref().map_or(1, Vec::len);
        (1, triangles * instances)
    });
    for child in &node.children {
        let (d, t) = count(child);
        totals.0 += d;
        totals.1 += t;
    }
    totals
}
use sloppy_core::sim::house_layout::{REFERENCE_HOUSE, SENDA_STYLES};
use sloppy_core::sim::house_structure::house_structure;
use sloppy_core::sim::senda_layout::senda_houses;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = std::path::PathBuf::from(
        args.get(1)
            .map_or("artifacts/performance/blender-house", |s| s),
    );
    std::fs::create_dir_all(&out).unwrap();
    let home = senda_houses()
        .into_iter()
        .find(|house| {
            SENDA_STYLES[usize::from(house.plan.style)]
                == SENDA_STYLES[usize::from(REFERENCE_HOUSE)]
        })
        .expect("the reference house");
    let design = home.design();
    let roof = design.roof();
    println!(
        "home at x {:.2} z {:.2}: w {:.2} d {:.2} h {:.2} turns {} angle {:.3} yaw {:.3} door_wall {} garage {:?}",
        home.x,
        home.z,
        design.w,
        design.d,
        design.h,
        design.turns,
        design.angle,
        design.yaw(),
        design.door_wall,
        design.garage
    );
    println!("roof {roof:?}");
    for index in 0..4 {
        println!("wall {index} plan {:?}", design.wall_plan(index));
        println!("wall {index} coded {:?}", design.coded_wall_plan(index));
    }
    println!(
        "ground window {:?} upper window {:?} door {}",
        roof.ground_window(),
        roof.upper_window(),
        roof.door_height()
    );
    let (obj, counts) = coded_pieces_obj(&design, home.x, home.z);
    std::fs::write(out.join("coded-home.obj"), obj).unwrap();
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    println!("coded pieces triangles {total}: {counts:?}");
    let structure = house_structure(&design);
    let mut parts = Vec::new();
    for part in &structure.parts {
        let role = part.house();
        parts.push(serde_json::json!({
            "kind": format!("{:?}", role.kind),
            "wall": role.wall,
            "upper": role.upper_storey,
            "min": [part.min.x, part.min.y, part.min.z],
            "max": [part.max.x, part.max.y, part.max.z],
            "cells": role.cells.iter().map(|c| serde_json::json!({"part": format!("{:?}", c.part), "min": [c.min.x, c.min.y], "max": [c.max.x, c.max.y]})).collect::<Vec<_>>(),
        }));
    }
    println!("structure parts {}", structure.parts.len());
    let info = serde_json::json!({
        "x": home.x, "z": home.z, "w": design.w, "d": design.d, "h": design.h, "yaw": design.yaw(),
        "roof": {"wall": roof.wall, "ridge": roof.ridge, "pitch": roof.pitch, "eave_x": roof.eave_x, "rake_z": roof.rake_z, "eave_y": roof.eave_y},
        "parts": parts,
    });
    std::fs::write(
        out.join("coded-home.json"),
        serde_json::to_string_pretty(&info).unwrap(),
    )
    .unwrap();
    let measure = |label: &str| {
        // Standing (whole) and with a third of its parts broken (the cut pieces).
        let mut broken = PartSet::default();
        for index in (0..structure.parts.len()).step_by(3) {
            broken.insert(index);
        }
        for (name, set) in [("standing", PartSet::default()), ("damaged", broken)] {
            let start = std::time::Instant::now();
            let runs = 5;
            let mut group = Node::group("");
            for _ in 0..runs {
                group = Node::group("");
                structural_house(
                    &mut group,
                    home.x,
                    home.z,
                    home.w,
                    home.d,
                    home.h,
                    Some(home.plan),
                    &set,
                );
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
            let (draws, triangles) = count(&group);
            println!("{label} {name}: {draws} draws, {triangles} triangles, build {ms:.2} ms");
        }
    };
    measure("coded");
    if let Some(glb) = args.get(2) {
        let bytes = std::fs::read(glb).unwrap();
        let start = std::time::Instant::now();
        let house = parse_glb(&bytes).unwrap();
        println!("parse {:.2} ms", start.elapsed().as_secs_f64() * 1000.0);
        install("small-monterey", &bytes).unwrap();
        measure("imported");
        let mut by_part = std::collections::BTreeMap::<String, usize>::new();
        let mut finishes = std::collections::BTreeSet::new();
        for (part, surface, triangles) in house.summary() {
            *by_part.entry(part).or_default() += triangles;
            finishes.insert(surface);
        }
        println!(
            "imported triangles {}: {by_part:?}; finishes {finishes:?}",
            by_part.values().sum::<usize>()
        );
    }
}
