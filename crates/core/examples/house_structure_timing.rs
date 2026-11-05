//! What a structural cottage costs natively:
//! `cargo run -p sloppy-core --release --example house_structure_timing`.
//! Prints the village cottages' part and collider counts, the time to lay one out, to
//! break one wall panel (with the support recompute and any fall), to burst an HE
//! shell inside one, to level one from its footing, and a world step with the village's
//! fourteen cottages standing against one with their bodies taken out. Manual evidence,
//! not a CI gate.

use std::time::Instant;

use sloppy_core::sim::house_physics::{HE_BLAST_RADIUS, house_structure};
use sloppy_core::sim::house_structure::{PartKind, PartSet};
use sloppy_core::sim::math::Point3;
use sloppy_core::sim::{CoverKind, Simulation};

const ROUNDS: usize = 200;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn summary(samples: Vec<f64>) -> String {
    let count = samples.len();
    let max = samples.iter().copied().fold(0.0, f64::max);
    format!(
        "median {:.3} ms, max {:.3} ms over {count}",
        median(samples),
        max
    )
}

fn main() {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let houses: Vec<usize> = (0..s.covers.len())
        .filter(|&i| s.covers[i].kind == CoverKind::House)
        .collect();
    let parts: Vec<usize> = houses
        .iter()
        .map(|&i| s.covers[i].structure.as_ref().unwrap().layout.parts.len())
        .collect();
    let colliders: usize = houses
        .iter()
        .map(|&i| s.world.bodies[s.covers[i].body].colliders().len())
        .sum();
    println!(
        "cottages {}, parts per cottage {}..{} (total {}), cottage colliders {colliders}, world colliders {}",
        houses.len(),
        parts.iter().min().unwrap(),
        parts.iter().max().unwrap(),
        parts.iter().sum::<usize>(),
        s.world.colliders.len()
    );

    let layout_ms: Vec<f64> = (0..ROUNDS)
        .map(|k| {
            let c = &s.covers[houses[k % houses.len()]];
            let start = Instant::now();
            let layout = house_structure(c.x, c.z, c.w, c.d, c.h);
            std::hint::black_box(layout);
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    println!("lay out one cottage: {}", summary(layout_ms));

    // Break each lower wall panel in turn (rebuilding the cottage between), so the
    // support recompute runs on a whole cottage each time.
    let (mut break_ms, mut blast_ms, mut fell_ms) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..ROUNDS {
        let house = houses[k % houses.len()];
        let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
        let panels: Vec<usize> = (0..layout.parts.len())
            .filter(|&i| layout.parts[i].kind == PartKind::Panel && layout.parts[i].grounded)
            .collect();
        let panel = panels[k % panels.len()];
        let start = Instant::now();
        s.break_house_parts(house, PartSet::from_indices([panel]), None, None);
        break_ms.push(start.elapsed().as_secs_f64() * 1e3);
        // An HE burst a metre inside that hole.
        let c = &s.covers[house];
        let part = &layout.parts[panel];
        let inside = layout.design.to_world(part.center() * 0.6);
        let point = Point3::new(c.x + inside.x, 1.0, c.z + inside.z);
        let start = Instant::now();
        s.house_blast(point, HE_BLAST_RADIUS, 40.0, None);
        blast_ms.push(start.elapsed().as_secs_f64() * 1e3);
        // Its footing gone: everything above falls as bodies.
        let footing =
            PartSet::from_indices((0..layout.parts.len()).filter(|&i| {
                layout.parts[i].grounded && layout.parts[i].kind != PartKind::Chimney
            }));
        let start = Instant::now();
        s.break_house_parts(house, footing, None, None);
        fell_ms.push(start.elapsed().as_secs_f64() * 1e3);
        // Rebuild it for the next round, with its fallen bodies cleared.
        s.covers[house].alive = false;
        s.restore_cover(house);
    }
    println!(
        "break one wall panel (support recompute): {}",
        summary(break_ms)
    );
    println!(
        "HE burst inside (blast with shielding rays): {}",
        summary(blast_ms)
    );
    println!(
        "level one from its footing (falling bodies): {}",
        summary(fell_ms)
    );

    // A world step with every cottage standing, then without them.
    let step = |s: &mut Simulation| {
        let mut samples = Vec::new();
        for _ in 0..ROUNDS {
            let start = Instant::now();
            s.world.step();
            samples.push(start.elapsed().as_secs_f64() * 1e3);
        }
        summary(samples)
    };
    println!("world step, cottages standing: {}", step(&mut s));
    for &house in &houses {
        let body = s.covers[house].body;
        s.remove_body(body);
    }
    println!("world step, cottages taken out: {}", step(&mut s));
}
