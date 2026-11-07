//! Heap allocations and time of single player's page work per frame, without a GPU:
//! what `Game::frame` does around the simulation step and the renderer, and what the shell
//! reads after it. `cargo run --release -p sloppy-web --example page_frame_benchmark`
//!
//! A seeded autoplay match per map steps once per 60 Hz frame; each frame then copies the
//! render state (`fill_render_state`), routes the step's events with their feedback
//! (`event_feedback`, as `Game::route_events`, less presentation's share) and, as the
//! shell does, takes the event batch with its sounds when there are events and builds the
//! HUD every `HUD_UPDATE_EVERY_FRAMES` frames. The step itself is left out (the simulation
//! benchmarks count it). A counting global allocator tallies allocations and requested
//! bytes in the page work only. The HUD and event batches are hashed as JSON outside the
//! count, so two builds that print the same hashes produced the same page output.
//! Manual evidence, not a CI gate.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hash::{DefaultHasher, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::{
    MatchPhase, RenderState, SimEventType, Simulation, SimulationSetup, VehicleCommand,
};
use sloppy_web::events::{PendingEvent, take_events};
use sloppy_web::feedback::event_feedback;
use sloppy_web::hud::{EffectTimers, Scoreboard, simulation_hud};
use sloppy_web::sounds::SoundPlanner;

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

fn record(bytes: usize) {
    if COUNTING.load(Relaxed) {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(bytes as u64, Relaxed);
    }
}

// Delegates each allocation to System with the original pointer/layout unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const SEED: f64 = 79.0;
const WARMUP_FRAMES: usize = 600;
const MEASURED_FRAMES: usize = 3600;
const FRAME_MS: f64 = 1000.0 / 60.0;
/// As the page's `HUD_UPDATE_EVERY_FRAMES`.
const HUD_EVERY_FRAMES: usize = 4;
const MAPS: [MapId; 5] = [
    MapId::Village,
    MapId::Harbor,
    MapId::Quarry,
    MapId::StressTest,
    MapId::Superstress,
];

/// The page's share of one frame, as `Game` keeps it.
struct Page {
    state: RenderState,
    events: Vec<PendingEvent>,
    sounds: SoundPlanner,
    hud_tank: RenderTank,
    timers: EffectTimers,
}

impl Page {
    /// `Game::route_events` without presentation's cue and damage angle.
    fn route_events(&mut self, sim: &mut Simulation) {
        if sim.events.is_empty() {
            return;
        }
        let human = sim.human();
        let (human_id, human_team) = (human.id, human.team);
        let mut events = std::mem::take(&mut sim.events);
        for event in events.drain(..) {
            let player_hit = matches!(event.kind, SimEventType::Hurt | SimEventType::Death)
                && event.owner == Some(human_id)
                && event.team != Some(human_team);
            let own = event.id == Some(human_id);
            let scoreboard = Scoreboard::Simulated(&sim.tanks);
            let feedback = event_feedback(&event, own, human_id, &scoreboard);
            self.events.push(PendingEvent {
                event,
                player_hit,
                own,
                damage_angle: None,
                feedback,
            });
        }
        if sim.events.is_empty() {
            sim.events = events;
        }
    }
}

fn main() {
    for map in MAPS {
        let setup = SimulationSetup {
            map_mode: Some(map),
            ..SimulationSetup::default()
        }
        .merged(
            extra_level(map)
                .map(single_player_rules)
                .unwrap_or_default(),
        );
        let mut sim = Simulation::new(SEED, setup);
        sim.start();
        let mut page = Page {
            state: RenderState::default(),
            events: Vec::new(),
            sounds: SoundPlanner::default(),
            hud_tank: RenderTank::default(),
            timers: EffectTimers::default(),
        };
        let (mut allocations, mut bytes, mut most) = (0, 0, 0);
        let mut micros = Vec::with_capacity(MEASURED_FRAMES);
        let mut output = DefaultHasher::new();
        let mut frames = 0;
        for frame in 0..WARMUP_FRAMES + MEASURED_FRAMES {
            if sim.match_state.phase != MatchPhase::Playing {
                break;
            }
            sim.step(VehicleCommand::idle(), true);
            let now = frame as f64 * FRAME_MS;
            let measured = frame >= WARMUP_FRAMES;
            ALLOCATIONS.store(0, Relaxed);
            BYTES.store(0, Relaxed);
            COUNTING.store(measured, Relaxed);
            let start = Instant::now();
            sim.fill_render_state(&mut page.state, None);
            page.route_events(&mut sim);
            page.sounds.set_clock(now);
            let batch = (!page.events.is_empty()).then(|| {
                let listener = sim.tank_position(sim.human());
                take_events(
                    &mut page.events,
                    (listener.x, listener.z),
                    (1.0, 0.0),
                    &mut page.sounds,
                )
            });
            let hud = frame
                .is_multiple_of(HUD_EVERY_FRAMES)
                .then(|| simulation_hud(&sim, &mut page.hud_tank, None, &mut page.timers));
            let elapsed = start.elapsed().as_secs_f64() * 1e6;
            COUNTING.store(false, Relaxed);
            if let Some(batch) = &batch {
                output.write(serde_json::to_string(batch).unwrap().as_bytes());
            }
            if let Some(hud) = &hud {
                output.write(serde_json::to_string(hud).unwrap().as_bytes());
            }
            drop((batch, hud));
            if measured {
                let used = ALLOCATIONS.load(Relaxed);
                allocations += used;
                bytes += BYTES.load(Relaxed);
                most = most.max(used);
                micros.push(elapsed);
                frames += 1;
            }
        }
        assert!(frames > 0, "the match ended during warm-up");
        micros.sort_by(f64::total_cmp);
        let at = |fraction: f64| micros[((fraction * frames as f64) as usize).min(frames - 1)];
        println!(
            "{:<12} {frames} frames: {:5.2} allocs {:6.0} B per frame (max {most}), \
             {:.2} µs mean, p50 {:.2}, p95 {:.2}, max {:.2}; output {:016x}",
            map.as_str(),
            allocations as f64 / frames as f64,
            bytes as f64 / frames as f64,
            micros.iter().sum::<f64>() / frames as f64,
            at(0.5),
            at(0.95),
            micros[frames - 1],
            output.finish(),
        );
    }
}
