//! The mesh page planner (`mesh_pages.rs`) over whole sessions: a recorded Stress
//! Grid session replayed round after round, a tour of maps that each cache their
//! theme's models, and the endless model churn of a level that rebuilds its cover in
//! place and never resets. Pages appear as the live mesh data first needs them, and
//! the general pages left empty go at the next frame (`PagePlanner::trim`), after a
//! reset's new round has refilled what it could; the page set must settle, not keep
//! growing.

use sloppy_render::effects::random::CosmeticRandom;
use sloppy_render::mesh_pages::{PageFamily, PageKind, PagePlanner};

const SURFACE: PageFamily = PageFamily::Surface { extra: 0 };
const FAMILIES: [PageFamily; 3] = [SURFACE, PageFamily::Shadow, PageFamily::Index];

/// One mesh in the planner: its vertices and its indices.
#[derive(Clone, Copy)]
struct Placed {
    vertex_page: u16,
    first_vertex: u32,
    vertices: u32,
    index_page: u16,
    first_index: u32,
    indices: u32,
}

fn upload(planner: &mut PagePlanner, family: PageFamily, vertices: u32, indices: u32) -> Placed {
    let vertex = planner.place(family, vertices);
    let index = planner.place(PageFamily::Index, indices);
    Placed {
        vertex_page: vertex.page,
        first_vertex: vertex.first,
        vertices,
        index_page: index.page,
        first_index: index.first,
        indices,
    }
}

fn free(planner: &mut PagePlanner, mesh: Placed) {
    planner.free(mesh.vertex_page, mesh.first_vertex, mesh.vertices);
    planner.free(mesh.index_page, mesh.first_index, mesh.indices);
}

/// Every live page: id, family, kind and capacity.
type PageSet = Vec<(u16, PageFamily, PageKind, u32)>;

fn page_set(planner: &PagePlanner) -> PageSet {
    planner
        .pages()
        .map(|(id, page)| (id, page.family, page.kind, page.capacity()))
        .collect()
}

/// Bytes a family's general pages reserve, and the bytes of meshes in them.
fn general_bytes(planner: &PagePlanner, family: PageFamily) -> (u64, u64) {
    planner
        .pages()
        .filter(|(_, page)| page.family == family && page.kind == PageKind::General)
        .map(|(_, page)| (page.capacity(), page.live()))
        .fold((0, 0), |(capacity, live), page| {
            let bytes = family.element_bytes();
            (
                capacity + u64::from(page.0) * bytes,
                live + u64::from(page.1) * bytes,
            )
        })
}

fn general_page_bytes(family: PageFamily) -> u64 {
    u64::from(family.general_capacity()) * family.element_bytes()
}

// ------------------------------------------------------------------ recorded

enum Event {
    Upload {
        family: PageFamily,
        vertices: u32,
        indices: u32,
        keep: bool,
    },
    /// Uploads `first..=last`.
    Free(usize, usize),
    Round,
    End,
}

/// `fixtures/stress-mesh-uploads.txt`, which `fixtures/record-mesh-uploads.mjs`
/// records, with repeated uploads expanded.
fn recorded_session() -> Vec<Event> {
    let mut events = Vec::new();
    for line in include_str!("fixtures/stress-mesh-uploads.txt").lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [] | ["#", ..] => {}
            ["round"] => events.push(Event::Round),
            ["end"] => events.push(Event::End),
            ["-", ids] => {
                let (first, last) = ids.split_once('-').unwrap_or((ids, ids));
                events.push(Event::Free(first.parse().unwrap(), last.parse().unwrap()));
            }
            ["+", family, vertices, indices, rest @ ..] => {
                let family = match *family {
                    "S" => SURFACE,
                    "H" => PageFamily::Shadow,
                    other => panic!("unknown family {other}"),
                };
                let repeats = rest
                    .iter()
                    .find_map(|word| word.strip_prefix('*'))
                    .map_or(1, |n| n.parse().unwrap());
                for _ in 0..repeats {
                    events.push(Event::Upload {
                        family,
                        vertices: vertices.parse().unwrap(),
                        indices: indices.parse().unwrap(),
                        keep: rest.contains(&"keep"),
                    });
                }
            }
            _ => panic!("unreadable fixture line: {line}"),
        }
    }
    events
}

/// A replayed round: its pages just before it ends and after, and per family, the
/// most bytes of meshes its general pages held and the bytes they reserve.
struct Round {
    playing: PageSet,
    ended: PageSet,
    most_live: [u64; 3],
    capacity: [u64; 3],
}

#[test]
fn recorded_rounds_settle_on_one_page_set() {
    let events = recorded_session();
    let uploads = events
        .iter()
        .filter(|event| matches!(event, Event::Upload { .. }))
        .count();
    let round_at = events
        .iter()
        .position(|event| matches!(event, Event::Round))
        .unwrap();
    // Each upload is placed on its own, without batch pages: the recording does not
    // say which uploads one registration made.
    let mut planner = PagePlanner::default();
    let mut meshes: Vec<Option<Placed>> = vec![None; uploads];
    let mut kept = vec![false; uploads];
    let mut next = 0;
    // The menu and the map load once.
    for event in &events[..round_at] {
        match *event {
            Event::Upload {
                family,
                vertices,
                indices,
                ..
            } => {
                meshes[next] = Some(upload(&mut planner, family, vertices, indices));
                next += 1;
            }
            Event::Free(first, last) => {
                for mesh in &mut meshes[first..=last] {
                    free(&mut planner, mesh.take().unwrap());
                }
            }
            Event::Round | Event::End => unreachable!(),
        }
    }
    let first_round_upload = next;
    let mut rounds: Vec<Round> = Vec::new();
    for round in 0..4 {
        let mut next = first_round_upload;
        let mut playing = Vec::new();
        let mut most_live = [0; 3];
        // Whether meshes were placed since the last frees. Frees that follow uploads
        // come a frame later, and that frame's collection trimmed first. A round
        // starts with none: the reset's frees and its uploads share one frame.
        let mut uploaded = false;
        for event in &events[round_at + 1..] {
            match *event {
                Event::Upload {
                    family,
                    vertices,
                    indices,
                    keep,
                } => {
                    // Models the first round cached stay for the session.
                    if round == 0 || !keep {
                        meshes[next] = Some(upload(&mut planner, family, vertices, indices));
                        kept[next] = keep;
                        for (most, family) in most_live.iter_mut().zip(FAMILIES) {
                            *most = (*most).max(general_bytes(&planner, family).1);
                        }
                        uploaded = true;
                    }
                    next += 1;
                }
                Event::Free(first, last) => {
                    if std::mem::take(&mut uploaded) {
                        planner.trim();
                    }
                    // Menu meshes go only at the first round start.
                    for mesh in meshes[first..=last].iter_mut().filter_map(Option::take) {
                        free(&mut planner, mesh);
                    }
                }
                Event::End => {
                    // The reset frees the round's meshes; the next round's uploads
                    // refill the general pages that empties before a frame trims.
                    playing = page_set(&planner);
                    for id in first_round_upload..uploads {
                        if let (false, Some(mesh)) = (kept[id], meshes[id].take()) {
                            free(&mut planner, mesh);
                        }
                    }
                }
                Event::Round => unreachable!(),
            }
        }
        rounds.push(Round {
            playing,
            ended: page_set(&planner),
            most_live,
            capacity: FAMILIES.map(|family| general_bytes(&planner, family).0),
        });
    }
    // Holes and the newest page's free tail never add up to a whole general page
    // beyond the most a family's general pages held: a page is added only when the
    // live data needs it.
    for (index, round) in rounds.iter().enumerate() {
        for (family, (&capacity, &most)) in FAMILIES
            .iter()
            .zip(round.capacity.iter().zip(&round.most_live))
        {
            assert!(
                capacity < most + general_page_bytes(*family),
                "round {index} {family:?}: {capacity} bytes of general pages for at most {most}"
            );
        }
    }
    // Every round from the second on uses exactly the same pages. (The second round
    // may differ from the first: the models the first round cached are already there
    // while its meshes come and go.)
    for (index, round) in rounds.iter().enumerate().skip(2) {
        assert_eq!(round.playing, rounds[1].playing, "round {index} playing");
        assert_eq!(round.ended, rounds[1].ended, "round {index} ended");
    }
}

// ------------------------------------------------------------------ map tour

/// Models each map's theme caches for the session on its first visit (movers, water,
/// smoke), and the models one round on each map builds: maps differ in size.
const THEME_MODELS: usize = 12;
const ROUND_MODELS: [usize; 3] = [140, 40, 90];

#[test]
fn a_map_tour_keeps_only_the_pages_the_current_map_needs() {
    let mut random = CosmeticRandom::seeded(31);
    let mut models = |count: usize| -> Vec<Vec<(u32, u32)>> {
        (0..count).map(|_| model_kind(&mut random)).collect()
    };
    let themes: Vec<_> = ROUND_MODELS.iter().map(|_| models(THEME_MODELS)).collect();
    let rounds: Vec<_> = ROUND_MODELS.iter().map(|&count| models(count)).collect();
    let mut planner = PagePlanner::default();
    for _ in 0..300 {
        let (vertices, indices) = mesh_size(&mut random);
        upload(&mut planner, SURFACE, vertices, indices);
    }
    let mut cached = [false; ROUND_MODELS.len()];
    let mut round: Vec<Placed> = Vec::new();
    let mut playing: Vec<Vec<PageSet>> = vec![Vec::new(); ROUND_MODELS.len()];
    for cycle in 0..4 {
        for map in 0..ROUND_MODELS.len() {
            // A reset frees the last round's models; a map's first visit then
            // caches its theme's models and the round builds its own, refilling the
            // general pages the old round emptied (`View::reset`), and the next
            // frame trims the ones left empty.
            for mesh in round.drain(..) {
                free(&mut planner, mesh);
            }
            if !cached[map] {
                for kind in &themes[map] {
                    add_model(&mut planner, kind);
                }
                cached[map] = true;
            }
            for kind in &rounds[map] {
                round.extend(add_model(&mut planner, kind));
            }
            planner.trim();
            // While a round plays, each family's general pages hold its meshes plus
            // less than one page: the newest page's free tail and the holes between
            // cached models, not the pages a larger map or the last round needed.
            for family in FAMILIES {
                let (capacity, live) = general_bytes(&planner, family);
                assert!(
                    capacity < live + general_page_bytes(family),
                    "cycle {cycle} map {map} {family:?}: {capacity} bytes of general pages for {live}"
                );
            }
            playing[map].push(page_set(&planner));
        }
    }
    // From the second visit on, a map's rounds use the same pages every time.
    for (map, sets) in playing.iter().enumerate() {
        for (cycle, set) in sets.iter().enumerate().skip(2) {
            assert_eq!(set, &sets[1], "map {map} cycle {cycle}");
        }
    }
}

// ------------------------------------------------------------------ churn

/// Distinct models the level builds from, models on the level, and replacements.
const MODEL_KINDS: usize = 60;
const LEVEL_MODELS: usize = 150;
const REPLACEMENTS: usize = 10_000;

/// A mesh between 4 and 40,000 vertices, small far more often than large, with one
/// to three indices per vertex.
fn mesh_size(random: &mut CosmeticRandom) -> (u32, u32) {
    let vertices = (4.0 * 10_000f64.powf(random.next_f64())) as u32;
    let indices = vertices * (1 + (random.next_f64() * 3.0) as u32);
    (vertices, indices)
}

/// A kind of model: the sizes of its one to six surface meshes. Its shadow merge
/// holds them all.
fn model_kind(random: &mut CosmeticRandom) -> Vec<(u32, u32)> {
    let parts = 1 + (random.next_f64() * 6.0) as usize;
    (0..parts).map(|_| mesh_size(random)).collect()
}

fn add_model(planner: &mut PagePlanner, kind: &[(u32, u32)]) -> Vec<Placed> {
    let mut meshes: Vec<Placed> = kind
        .iter()
        .map(|&(vertices, indices)| upload(planner, SURFACE, vertices, indices))
        .collect();
    let vertices = kind.iter().map(|size| size.0).sum();
    let indices = kind.iter().map(|size| size.1).sum();
    meshes.push(upload(planner, PageFamily::Shadow, vertices, indices));
    meshes
}

#[test]
fn endless_model_replacement_keeps_the_page_set_bounded() {
    let mut random = CosmeticRandom::seeded(207);
    let kinds: Vec<Vec<(u32, u32)>> = (0..MODEL_KINDS).map(|_| model_kind(&mut random)).collect();
    let pick = |random: &mut CosmeticRandom| {
        let at = (random.next_f64() * MODEL_KINDS as f64) as usize;
        &kinds[at]
    };
    let mut planner = PagePlanner::default();
    // Session meshes, then a level of models that never resets, where one random
    // model after another is rebuilt as a random kind, like cover rebuilt in place.
    for _ in 0..300 {
        let (vertices, indices) = mesh_size(&mut random);
        upload(&mut planner, SURFACE, vertices, indices);
    }
    let mut models: Vec<Vec<Placed>> = (0..LEVEL_MODELS)
        .map(|_| add_model(&mut planner, pick(&mut random)))
        .collect();
    let mut most_live = planner.live_bytes();
    let mut most_pages = planner.pages().count();
    let mut largest_id = 0;
    for _ in 0..REPLACEMENTS {
        let at = (random.next_f64() * LEVEL_MODELS as f64) as usize;
        for mesh in std::mem::take(&mut models[at]) {
            free(&mut planner, mesh);
        }
        models[at] = add_model(&mut planner, pick(&mut random));
        most_live = most_live.max(planner.live_bytes());
        most_pages = most_pages.max(planner.pages().count());
        largest_id = largest_id.max(planner.pages().map(|(id, _)| id).max().unwrap());
    }
    // Own pages come and go (large shadow merges), and their ids are reused.
    assert!(usize::from(largest_id) < most_pages);
    // The pages track the most live data, with at most a quarter as much again in
    // holes and partly filled pages, plus a general page per family.
    let pages: u64 = FAMILIES.map(general_page_bytes).iter().sum();
    assert!(
        planner.capacity_bytes() <= most_live * 5 / 4 + pages,
        "{} MiB of pages for at most {} MiB of meshes",
        planner.capacity_bytes() >> 20,
        most_live >> 20
    );
}
