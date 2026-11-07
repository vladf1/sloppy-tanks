//! What laying out and tessellating a screen costs the CPU each frame, over the
//! snapshot scenarios: allocations, bytes and microseconds per frame, and a hash of the
//! tessellated output so a change can show it draws the same. Ignored by default; run it
//! alone (the allocation counter is the whole test binary's) on an optimized build:
//! `cargo test --profile desktop -p sloppy-desktop frame_cost -- --ignored --nocapture`.
//!
//! The hash covers every measured frame's clip rects, textures, indices and vertices, so
//! equal hashes from two builds of the same profile mean they drew the same pixels.
//! `SLOPPY_FRAME_COST_ONLY=<label>` runs one scenario and `SLOPPY_FRAME_COST_FRAMES=<n>`
//! measures more frames (for a profiler). `SLOPPY_ALLOC_TRACE=<label>` prints where the
//! scenario's allocations of one frame come from, best on the `desktop-check` profile,
//! whose backtraces name the shell's lines (`SLOPPY_ALLOC_TRACE_CALLERS=1` names the
//! callers of `paint.rs` instead).

use std::alloc::{GlobalAlloc, Layout, System};
use std::backtrace::Backtrace;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sloppy_core::sim::MatchPhase;

use super::snapshots::{Scenario, scenarios};
use super::{View, touch::TouchView};
use crate::preferences::{Preferences, TouchSetting};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards every call to the system allocator unchanged, only counting.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        trace();
        // SAFETY: the caller's layout, as `System` requires.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `alloc` or `realloc` above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        trace();
        // SAFETY: as for `dealloc`, with the caller's new size.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Whether allocations record their backtraces, and those recorded.
static TRACING: AtomicBool = AtomicBool::new(false);
static TRACES: Mutex<Vec<Backtrace>> = Mutex::new(Vec::new());
thread_local! {
    /// Recording one: its own allocations are not recorded.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

fn trace() {
    if !TRACING.load(Ordering::Relaxed) || INSIDE.with(Cell::get) {
        return;
    }
    INSIDE.with(|inside| inside.set(true));
    let backtrace = Backtrace::force_capture();
    TRACES.lock().expect("the traces").push(backtrace);
    INSIDE.with(|inside| inside.set(false));
}

/// Print the recorded allocations' sites, the most frequent first: the shell's line
/// that made each, and the outside function that allocated.
fn report_traces() {
    let traces = std::mem::take(&mut *TRACES.lock().expect("the traces"));
    let mut sites = std::collections::BTreeMap::<String, u32>::new();
    for trace in &traces {
        let text = trace.to_string();
        // (function, location) per frame.
        let mut frames: Vec<(String, String)> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(location) = line.strip_prefix("at ") {
                if let Some(last) = frames.last_mut() {
                    last.1 = location.to_owned();
                }
            } else if let Some((_, name)) = line.split_once(": ") {
                frames.push((name.to_owned(), String::new()));
            }
        }
        let after_allocator = frames
            .iter()
            .position(|(name, _)| name.starts_with("__rust_"))
            .map_or(0, |index| index + 1);
        let external = frames[after_allocator..]
            .iter()
            .find(|(name, _)| {
                !(name.contains("alloc::")
                    || name.contains("RawVec")
                    || name.starts_with("core::")
                    || name.contains("hashbrown"))
            })
            .map_or("?", |(name, _)| name.as_str());
        let ours = frames[after_allocator..].iter().find(|(_, location)| {
            location.starts_with("./src/")
                && !location.contains("frame_cost")
                && (std::env::var_os("SLOPPY_ALLOC_TRACE_CALLERS").is_none()
                    || !location.contains("paint.rs"))
        });
        let key = match ours {
            Some((name, location)) => format!("{name} {location}  [{external}]"),
            None => format!("(egui) {external}"),
        };
        let key: String = key.chars().take(220).collect();
        *sites.entry(key).or_default() += 1;
    }
    let mut sites: Vec<_> = sites.into_iter().collect();
    sites.sort_by_key(|site| std::cmp::Reverse(site.1));
    for (site, count) in sites.iter().take(70) {
        println!("{count:>6}  {site}");
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const SIZE: [f32; 2] = [1280.0, 800.0];
const SCALE: f32 = 2.0;
const WARM_UP: u32 = 60;
const FRAMES: u32 = 600;

/// `SLOPPY_FRAME_COST_FRAMES`: more measured frames, for a profiler.
fn frames() -> u32 {
    std::env::var("SLOPPY_FRAME_COST_FRAMES")
        .ok()
        .and_then(|frames| frames.parse().ok())
        .unwrap_or(FRAMES)
}

/// `SLOPPY_FRAME_COST_ONLY`: the one scenario to run, by its printed label.
fn wanted(label: &str) -> bool {
    std::env::var("SLOPPY_FRAME_COST_ONLY").map_or(true, |only| only == label)
}

/// Frames between restoring the scenario's feedback, which the HUD's timers age and
/// fade meanwhile, as in a fight.
const FEEDBACK_CYCLE: u32 = 150;

fn hash_primitives(primitives: &[egui::ClippedPrimitive], hasher: &mut impl Hasher) {
    for primitive in primitives {
        let clip = primitive.clip_rect;
        for value in [clip.min.x, clip.min.y, clip.max.x, clip.max.y] {
            value.to_bits().hash(hasher);
        }
        match &primitive.primitive {
            egui::epaint::Primitive::Mesh(mesh) => {
                mesh.texture_id.hash(hasher);
                mesh.indices.hash(hasher);
                for vertex in &mesh.vertices {
                    for value in [vertex.pos.x, vertex.pos.y, vertex.uv.x, vertex.uv.y] {
                        value.to_bits().hash(hasher);
                    }
                    vertex.color.to_array().hash(hasher);
                }
            }
            egui::epaint::Primitive::Callback(_) => "callback".hash(hasher),
        }
    }
}

/// Run `scenario` for the warm-up and then the measured frames, and print its costs.
fn measure(label: &str, mut scenario: Scenario, touch: bool) {
    if !wanted(label) {
        return;
    }
    let frames = frames();
    let prefs = Preferences {
        debug: false,
        player_name: "Swift Falcon".into(),
        ..Preferences::default()
    };
    let is_room = scenario.name.starts_with("room-");
    let mut room_menu = is_room.then(|| super::room_menu::snapshot_menu(scenario.name));
    if touch && scenario.phase == MatchPhase::Playing {
        let alive = scenario
            .state
            .hud
            .as_ref()
            .is_some_and(|hud| hud.human.alive);
        scenario.state.touch = Some(TouchView {
            input: Default::default(),
            first_person: scenario.state.cockpit,
            learned: false,
            acting: alive,
        });
    }
    let context = egui::Context::default();
    super::theme::install(&context);
    let view = View {
        in_round: scenario.in_round,
        phase: scenario.phase,
        prefs: &prefs,
        build: "v0.1.0 · dev",
        debug: scenario.name.ends_with("nerd-stats"),
        room: is_room,
    };
    let feedback = (
        scenario.state.kill_feed.clone(),
        scenario.state.toast.clone(),
        scenario.state.ammo_notice.clone(),
        scenario.state.hit_flash,
        scenario.state.damage_ring,
    );
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut micros = Vec::with_capacity(frames as usize);
    let (mut allocations, mut bytes, mut texture_sets, mut vertices) = (0, 0, 0, 0);
    let (mut draws, mut font_draws) = (0, 0);
    for frame in 0..WARM_UP + frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(SIZE[0], SIZE[1]),
            )),
            time: Some(10.0 + f64::from(frame) / 60.0),
            ..Default::default()
        };
        let measured = frame >= WARM_UP;
        if frame % FEEDBACK_CYCLE == 0 {
            let state = &mut scenario.state;
            state.kill_feed.clone_from(&feedback.0);
            state.toast.clone_from(&feedback.1);
            state.ammo_notice.clone_from(&feedback.2);
            (state.hit_flash, state.damage_ring) = (feedback.3, feedback.4);
        } else if scenario.phase == MatchPhase::Playing {
            scenario.state.tick(1.0 / 60.0);
        }
        let start_allocations = ALLOCATIONS.load(Ordering::Relaxed);
        let start_bytes = BYTES.load(Ordering::Relaxed);
        let traced = frame == WARM_UP + 1
            && std::env::var("SLOPPY_ALLOC_TRACE").is_ok_and(|name| name == label);
        TRACING.store(traced, Ordering::Relaxed);
        let start = std::time::Instant::now();
        context.set_pixels_per_point(SCALE);
        let mut output = context.run_pass(input, |context| {
            if let Some(menu) = &mut room_menu {
                super::room_menu::draw(context, menu, &mut scenario.state, &view, 0.0);
            } else {
                super::draw(context, &mut scenario.state, &view);
            }
        });
        let textures = output.textures_delta.set.len();
        // No GPU here to apply them to.
        output.textures_delta.clear();
        let primitives = context.tessellate(output.shapes, SCALE);
        let elapsed = start.elapsed();
        TRACING.store(false, Ordering::Relaxed);
        if traced {
            report_traces();
        }
        if measured {
            allocations += ALLOCATIONS.load(Ordering::Relaxed) - start_allocations;
            bytes += BYTES.load(Ordering::Relaxed) - start_bytes;
            micros.push(elapsed.as_secs_f64() * 1e6);
            texture_sets += textures;
            hash_primitives(&primitives, &mut hasher);
            draws = primitives.len();
            font_draws = primitives
                .iter()
                .filter(|primitive| matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == egui::TextureId::default()))
                .count();
            vertices = primitives
                .iter()
                .map(|primitive| match &primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    egui::epaint::Primitive::Callback(_) => 0,
                })
                .sum::<usize>();
        }
    }
    micros.sort_by(f64::total_cmp);
    let frames = f64::from(frames);
    let mean = micros.iter().sum::<f64>() / frames;
    println!(
        "{label:<34} allocs/frame {:>8.1}  KB/frame {:>8.1}  µs median {:>7.1} mean {:>7.1} p90 {:>7.1}  vertices {vertices:>6}  draws {draws:>3} ({font_draws:>3} font)  texture sets {texture_sets}  hash {:016x}",
        allocations as f64 / frames,
        bytes as f64 / frames / 1024.0,
        micros[micros.len() / 2],
        mean,
        micros[micros.len() * 9 / 10],
        hasher.finish(),
    );
}

#[test]
#[ignore = "a measurement; run alone on an optimized build"]
fn frame_cost() {
    const DESKTOP: [&str; 12] = [
        "hud",
        "hud-solo-first-person-critical",
        "respawn",
        "pause",
        "results-team",
        "battle-setup-ready",
        "multiplayer-rooms",
        "hud-nerd-stats",
        "room-lobby",
        "room-results",
        "room-pause",
        "room-respawn",
    ];
    const TOUCH: [&str; 3] = ["hud", "hud-solo-first-person-critical", "respawn"];
    let take = |name: &str| {
        scenarios()
            .into_iter()
            .find(|scenario| scenario.name == name)
            .expect("a snapshot scenario")
    };
    for name in DESKTOP {
        measure(name, take(name), false);
    }
    // A tablet's touch controls (Settings' Touch controls on), then the phone edition.
    crate::touch::set_setting(TouchSetting::On);
    for name in TOUCH {
        measure(&format!("touch {name}"), take(name), true);
    }
    crate::touch::detect(true);
    for name in TOUCH.iter().chain(&["battle-setup-ready"]) {
        measure(&format!("phone {name}"), take(name), true);
    }
}
