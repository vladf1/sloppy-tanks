//! What a structural cottage costs natively:
//! `cargo run -p sloppy-core --release --example house_structure_timing`.
//! Prints the village cottages' part and collider counts, the time to lay one out, to
//! break one wall panel (with the support recompute and any fall), to burst an HE
//! shell inside one, to level one from its footing, to rebuild the biggest cottage's
//! model after a break (what the page does for each cottage a frame breaks), and a world
//! step with the village's fourteen cottages standing against one with their bodies
//! taken out. Manual evidence, not a CI gate.

use std::sync::Arc;
use std::time::Instant;

use sloppy_core::models::{HouseBuild, HouseDebris, house_debris, structural_house};
use sloppy_core::scene::Node;
use sloppy_core::sim::house_layout::HouseDesign;
use sloppy_core::sim::house_structure::{PartKind, house_structure};
use sloppy_core::sim::math::Point3;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::structure_physics::HE_BLAST_RADIUS;
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
            let layout = house_structure(&HouseDesign::of(c.x, c.z, c.w, c.d, c.h));
            std::hint::black_box(layout);
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    println!("lay out one cottage: {}", summary(layout_ms));

    // The biggest cottage's model, rebuilt as the page rebuilds it after a break: with
    // one panel gone, with a third of its parts gone, and a fallen slope's body.
    let biggest = *houses
        .iter()
        .max_by_key(|&&i| s.covers[i].structure.as_ref().unwrap().layout.parts.len())
        .unwrap();
    let c = s.covers[biggest].clone();
    let layout = c.structure.as_ref().unwrap().layout.clone();
    let panel = (0..layout.parts.len())
        .find(|&i| layout.parts[i].house().kind == PartKind::Panel && layout.parts[i].grounded)
        .unwrap();
    let third = PartSet::from_indices((0..layout.parts.len()).filter(|i| i % 3 == 0));
    let slope = PartSet::from_indices(
        (0..layout.parts.len()).filter(|&i| layout.parts[i].house().kind == PartKind::Sheet),
    );
    for (what, broken) in [
        ("whole", PartSet::EMPTY),
        ("one panel gone", PartSet::from_indices([panel])),
        ("a third gone", third),
    ] {
        let samples = (0..ROUNDS / 4)
            .map(|_| {
                let start = Instant::now();
                let mut group = Node::group("");
                structural_house(&mut group, c.x, c.z, c.w, c.d, c.h, None, &broken);
                std::hint::black_box(group);
                start.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        println!(
            "rebuild the biggest cottage's model ({} parts), {what}: {}",
            layout.parts.len(),
            summary(samples)
        );
        // The same a step at a time, as the page spreads it over frames.
        let mut steps = Vec::new();
        for _ in 0..ROUNDS / 4 {
            let layout = Arc::new(house_structure(&HouseDesign::of(c.x, c.z, c.w, c.d, c.h)));
            let mut build = HouseBuild::new(layout, c.x, c.z, &broken);
            loop {
                let start = Instant::now();
                let done = build.step();
                steps.push(start.elapsed().as_secs_f64() * 1e3);
                if done {
                    break;
                }
            }
            let start = Instant::now();
            std::hint::black_box(build.finish());
            steps.push(start.elapsed().as_secs_f64() * 1e3);
        }
        println!("  a step of it: {}", summary(steps));
    }
    let samples = (0..ROUNDS / 4)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(house_debris(c.x, c.z, c.w, c.d, c.h, None, &slope));
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    println!("build its fallen roof's body model: {}", summary(samples));
    // A collapse's bodies (a dozen here), each from its own pieces as the page built
    // them, against all of them cut from one build of the cottage's pieces.
    let bodies: Vec<PartSet> = (0..12)
        .map(|k| PartSet::from_indices((0..layout.parts.len()).filter(|i| i % 12 == k)))
        .collect();
    let each = (0..ROUNDS / 20)
        .map(|_| {
            let start = Instant::now();
            for parts in &bodies {
                std::hint::black_box(house_debris(c.x, c.z, c.w, c.d, c.h, None, parts));
            }
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    println!("  a dozen bodies, each its own pieces: {}", summary(each));
    let shared = (0..ROUNDS / 20)
        .map(|_| {
            let start = Instant::now();
            let layout = Arc::new(house_structure(&HouseDesign::of(c.x, c.z, c.w, c.d, c.h)));
            let pieces = HouseDebris::new(layout, c.x, c.z);
            for parts in &bodies {
                std::hint::black_box(pieces.body(parts));
            }
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    println!(
        "  a dozen bodies, one build of its pieces: {}",
        summary(shared)
    );

    // Break each lower wall panel in turn (rebuilding the cottage between), so the
    // support recompute runs on a whole cottage each time.
    let (mut break_ms, mut blast_ms, mut fell_ms) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..ROUNDS {
        let house = houses[k % houses.len()];
        let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
        let panels: Vec<usize> = (0..layout.parts.len())
            .filter(|&i| {
                layout.parts[i].house().kind == PartKind::Panel && layout.parts[i].grounded
            })
            .collect();
        let panel = panels[k % panels.len()];
        let start = Instant::now();
        s.break_structure(
            house,
            PartSet::from_indices([panel]),
            PartSet::EMPTY,
            None,
            None,
        );
        break_ms.push(start.elapsed().as_secs_f64() * 1e3);
        // An HE burst a metre inside that hole.
        let c = &s.covers[house];
        let part = &layout.parts[panel];
        let inside = layout.to_world(part.middle() * 0.6);
        let point = Point3::new(c.x + inside.x, 1.0, c.z + inside.z);
        let start = Instant::now();
        s.structure_blast(point, HE_BLAST_RADIUS, 40.0, None);
        blast_ms.push(start.elapsed().as_secs_f64() * 1e3);
        // Its footing gone: everything above falls as bodies.
        let footing = PartSet::from_indices((0..layout.parts.len()).filter(|&i| {
            layout.parts[i].grounded && layout.parts[i].house().kind != PartKind::Chimney
        }));
        let start = Instant::now();
        s.break_structure(house, footing, PartSet::EMPTY, None, None);
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
