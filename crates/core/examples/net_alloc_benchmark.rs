//! Heap allocations per tick of a room: a host and two page sessions wired in memory on
//! one clock, through a Village fight in which one player shells a cottage while a wall of
//! it is knocked out a part a tick, so the rest comes down around it.
//! `cargo run -p sloppy-core --release --example net_alloc_benchmark`
//!
//! A counting global allocator tallies every `alloc` and `realloc` (and their bytes) in:
//! the host's steps (seat commands and the simulation step, up to the tick hook), the
//! host's networking (path recording, events, capture, diff, encoding and batching, from
//! the hook to the end of `advance`), each client's receipt of binary state
//! (`socket_binary`: decode, apply, project, prediction and timeline) and each client's
//! page frames (`poll` and `frame`). The fight runs 2 s, then the cottage's wall goes and
//! the run continues 10 s more while its parts fall and settle. Manual evidence, not a CI
//! gate; the simulation's own allocations are counted in the host's steps.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sloppy_core::net::client::{
    ClientAction, ClientConfig, ClientNotice, LocalInput, NetworkClient,
};
use sloppy_core::net::fixed_step_clock::HOST_INTERVAL_MS;
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{JoinChoice, Message, RoomSettings};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::house_structure::PartKind;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::{CoverKind, VehicleKind};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Clone, Copy, Default)]
struct Count {
    allocations: u64,
    bytes: u64,
}

impl Count {
    fn now() -> Self {
        Self {
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            bytes: BYTES.load(Ordering::Relaxed),
        }
    }

    fn since(self, start: Count) -> Self {
        Self {
            allocations: self.allocations - start.allocations,
            bytes: self.bytes - start.bytes,
        }
    }

    fn add(&mut self, other: Count) {
        self.allocations += other.allocations;
        self.bytes += other.bytes;
    }
}

/// Totals of one stage over a window, with how many times it ran.
#[derive(Clone, Copy, Default)]
struct Tally {
    total: Count,
    runs: u64,
    most: u64,
}

impl Tally {
    fn record(&mut self, count: Count) {
        self.total.add(count);
        self.runs += 1;
        self.most = self.most.max(count.allocations);
    }

    fn line(&self, name: &str, per: &str) {
        let runs = self.runs.max(1) as f64;
        println!(
            "  {name:<20} {:>8.1} allocs {:>9.0} B per {per:<6} max {:>5} allocs  ({} {per}s)",
            self.total.allocations as f64 / runs,
            self.total.bytes as f64 / runs,
            self.most,
            self.runs,
        );
    }
}

const FRAME_MS: f64 = 1000.0 / 60.0;
const FIGHT_MS: f64 = 2000.0;
const COLLAPSE_MS: f64 = 10_000.0;

#[derive(Default)]
struct Window {
    host_step: Tally,
    host_net: Tally,
    client_receive: [Tally; 2],
    client_frame: [Tally; 2],
}

/// From which tick the hook knocks out the cottage's wall, a part a tick, and the parts.
type WallKnock = (u64, Vec<usize>);

/// Allocation marks the tick hook leaves: when it was entered and left, per tick.
struct HookMarks {
    entered: Vec<Count>,
    left: Vec<Count>,
}

struct Peer {
    client: NetworkClient,
    socket: Option<(u32, u64)>,
    opening: Vec<(u32, u64)>,
    input: LocalInput,
}

struct Network {
    host: MatchHost,
    peers: Vec<Peer>,
    now: f64,
    next_connection: u64,
    routes: BTreeMap<u64, (usize, u32)>,
    next_host_tick: f64,
    /// Where receipts and frames are counted, when measuring.
    window: Option<Window>,
    marks: Arc<Mutex<HookMarks>>,
    /// Every binary message the host sent, and every frame each page drew, hashed: two
    /// builds that print the same hashes sent the same bytes and drew the same scenes.
    sent: DefaultHasher,
    drawn: [DefaultHasher; 2],
}

impl Network {
    fn settle(&mut self) {
        loop {
            let mut busy = false;
            for index in 0..self.peers.len() {
                let actions = self.peers[index].client.take_actions();
                busy |= !actions.is_empty();
                for action in actions {
                    self.perform(index, action);
                }
                let notices = self.peers[index].client.take_notices();
                let now = self.now;
                for notice in notices {
                    match notice {
                        ClientNotice::PrepareArena {
                            round_id,
                            room_epoch,
                        } => self.peers[index]
                            .client
                            .arena_prepared(round_id, &room_epoch, now),
                        ClientNotice::ShowBaseline { baseline } => {
                            self.peers[index].client.baseline_shown(baseline, now)
                        }
                        _ => {}
                    }
                }
            }
            let events = self.host.take_events();
            busy |= !events.is_empty();
            for event in events {
                self.deliver(event);
            }
            if !busy {
                break;
            }
        }
    }

    fn perform(&mut self, index: usize, action: ClientAction) {
        let now = self.now;
        match action {
            ClientAction::OpenSocket { socket, .. } => {
                self.next_connection += 1;
                self.peers[index]
                    .opening
                    .push((socket, self.next_connection));
            }
            ClientAction::Send { socket, text } => {
                if let Some((current, connection)) = self.peers[index].socket
                    && current == socket
                {
                    self.host.receive(connection, &text, now as u64);
                }
            }
            ClientAction::CloseSocket { socket } => {
                if let Some((current, connection)) = self.peers[index].socket
                    && current == socket
                {
                    self.peers[index].socket = None;
                    self.routes.remove(&connection);
                    self.host.disconnect(connection, now as u64);
                    self.peers[index].client.socket_closed(socket, 1005, now);
                }
            }
            ClientAction::SaveSeat { .. } | ClientAction::ForgetSeat => {}
        }
    }

    fn deliver(&mut self, event: HostEvent) {
        let now = self.now;
        match event {
            HostEvent::Send {
                connection,
                message,
            } => {
                if let Some(&(peer, socket)) = self.routes.get(&connection) {
                    let client = &mut self.peers[peer].client;
                    match message {
                        Message::Text(text) => client.socket_message(socket, &text, now),
                        Message::Binary(bytes) => {
                            self.sent.write(&bytes);
                            let start = Count::now();
                            client.socket_binary(socket, &bytes, now);
                            let used = Count::now().since(start);
                            if let Some(window) = self.window.as_mut() {
                                window.client_receive[peer].record(used);
                            }
                        }
                    }
                }
            }
            HostEvent::Close {
                connection, code, ..
            } => {
                if let Some((peer, socket)) = self.routes.remove(&connection) {
                    self.peers[peer].socket = None;
                    self.host.disconnect(connection, now as u64);
                    self.peers[peer].client.socket_closed(socket, code, now);
                }
            }
            HostEvent::Changed => {}
        }
    }

    fn run(&mut self, ms: f64) {
        let end = self.now + ms;
        while self.now < end {
            self.now += FRAME_MS;
            let now = self.now;
            for index in 0..self.peers.len() {
                for (socket, connection) in std::mem::take(&mut self.peers[index].opening) {
                    self.peers[index].socket = Some((socket, connection));
                    self.routes.insert(connection, (index, socket));
                    self.peers[index].client.socket_opened(socket, now);
                }
                let input = self.peers[index].input;
                let start = Count::now();
                let peer = &mut self.peers[index];
                peer.client.poll(now);
                let frame = peer.client.frame(now, &input);
                let used = Count::now().since(start);
                if let Some(frame) = frame {
                    let drawn = &mut self.drawn[index];
                    drawn.write(
                        serde_json::to_string(frame.state)
                            .expect("a render state serializes")
                            .as_bytes(),
                    );
                    drawn.write(format!("{:?}", frame.events).as_bytes());
                }
                if let Some(window) = self.window.as_mut() {
                    window.client_frame[index].record(used);
                }
                self.peers[index].input.mine = false;
            }
            self.settle();
            while self.next_host_tick <= self.now {
                let at = self.next_host_tick as u64;
                {
                    let mut marks = self.marks.lock().unwrap();
                    marks.entered.clear();
                    marks.left.clear();
                }
                let start = Count::now();
                self.host.advance(at);
                let end = Count::now();
                // Copied after the count: the hook's lists keep their capacity, so it
                // allocates nothing of its own in the counted call.
                let (entered, left) = {
                    let marks = self.marks.lock().unwrap();
                    (marks.entered.clone(), marks.left.clone())
                };
                if let Some(window) = self.window.as_mut() {
                    // Each tick's step runs from the previous tick's hook (or the start).
                    let mut from = start;
                    let mut step = Count::default();
                    let mut hooks = Count::default();
                    for (entered, left) in entered.iter().zip(&left) {
                        step.add(entered.since(from));
                        hooks.add(left.since(*entered));
                        from = *left;
                    }
                    let mut net = end.since(start);
                    net.allocations -= step.allocations + hooks.allocations;
                    net.bytes -= step.bytes + hooks.bytes;
                    let ticks = entered.len() as u64;
                    if ticks > 0 {
                        let per_tick = |count: Count| Count {
                            allocations: count.allocations / ticks,
                            bytes: count.bytes / ticks,
                        };
                        for _ in 0..ticks {
                            window.host_step.record(per_tick(step));
                            window.host_net.record(per_tick(net));
                        }
                    }
                }
                let wait = self
                    .host
                    .until_next_batch_ms(at)
                    .map_or(HOST_INTERVAL_MS, |wait| wait.max(1.0));
                self.next_host_tick += wait.ceil();
                self.settle();
            }
        }
    }
}

fn choice(name: &str, create: Option<RoomSettings>) -> JoinChoice {
    JoinChoice {
        name: name.into(),
        kind: VehicleKind::Balanced,
        team: None,
        create,
        existing_room: None,
        bot: false,
    }
}

fn client() -> NetworkClient {
    NetworkClient::new(ClientConfig {
        server_url: "ws://127.0.0.1:8787/".into(),
        room: "ABCDEFGH".into(),
        saved_seat: None,
        delay: None,
        seed: 7,
        dodger_first: true,
        shooter_first: true,
    })
}

fn report(name: &str, window: &Window) {
    println!("{name}:");
    window.host_step.line("host step (sim)", "tick");
    window.host_net.line("host networking", "tick");
    for (index, peer) in ["alice", "bob"].iter().enumerate() {
        window.client_receive[index].line(&format!("{peer} receive"), "batch");
        window.client_frame[index].line(&format!("{peer} page frame"), "frame");
    }
}

fn main() {
    let mut token = 0;
    let host = MatchHost::new(MatchHostOptions {
        room_epoch: "epoch-1".into(),
        now_ms: 0,
        token: Box::new(move || {
            token += 1;
            format!("credential-{token:020}")
        }),
        seed: 4242,
        content_version: None,
    });
    let marks = Arc::new(Mutex::new(HookMarks {
        entered: Vec::with_capacity(16),
        left: Vec::with_capacity(16),
    }));
    let mut net = Network {
        host,
        peers: Vec::new(),
        now: 0.0,
        next_connection: 0,
        routes: BTreeMap::new(),
        next_host_tick: HOST_INTERVAL_MS.ceil(),
        window: None,
        marks: marks.clone(),
        sent: DefaultHasher::new(),
        drawn: [DefaultHasher::new(), DefaultHasher::new()],
    };
    for _ in 0..2 {
        net.peers.push(Peer {
            client: client(),
            socket: None,
            opening: Vec::new(),
            input: LocalInput::default(),
        });
    }
    let settings = RoomSettings {
        map_mode: MapId::Village,
        difficulty: Difficulty::Normal,
        humans_only: false,
        round_minutes: 5,
    };
    let now = net.now;
    net.peers[0]
        .client
        .connect(choice("alice", Some(settings)), now);
    net.settle();
    net.run(300.0);
    let now = net.now;
    net.peers[1].client.connect(choice("bob", None), now);
    net.settle();
    net.run(1000.0);

    // Alice faces the cottage nearest her from 16 m and shells it; bob drives about.
    let shooter = net.peers[0].client.control().expect("alice plays").tank_id;
    let (house, target) = {
        let sim = net.host.simulation.as_mut().expect("a round");
        let index = sim.tank_index(shooter).expect("alice's tank");
        let at = sim.body_translation(sim.tanks[index].body);
        let house = (0..sim.covers.len())
            .filter(|&i| sim.covers[i].kind == CoverKind::House)
            .min_by(|&a, &b| {
                let d = |i: usize| (sim.covers[i].x - at.x).hypot(sim.covers[i].z - at.z);
                d(a).total_cmp(&d(b))
            })
            .expect("a cottage");
        let target = Vec2::new(sim.covers[house].x, sim.covers[house].z);
        let body = sim.tanks[index].body;
        sim.world.bodies[body].set_translation(vector(target.x - 16.0, at.y, target.z), true);
        (house, target)
    };
    net.peers[0].input = LocalInput {
        fire: true,
        aim_point: Some((target.x, target.z)),
        ..LocalInput::default()
    };
    net.peers[1].input = LocalInput {
        move_z: 1.0,
        move_x: 0.4,
        fire: true,
        aim_angle: 1.0,
        ..LocalInput::default()
    };

    let hook_marks = marks.clone();
    let wall: Arc<Mutex<Option<WallKnock>>> = Arc::new(Mutex::new(None));
    let knock = wall.clone();
    net.host.tick_hook = Some(Box::new(move |sim, tick| {
        hook_marks.lock().unwrap().entered.push(Count::now());
        if let Some((start, parts)) = knock.lock().unwrap().as_mut() {
            let part = tick
                .checked_sub(*start)
                .and_then(|step| parts.get(step as usize).copied());
            if let Some(part) = part
                && sim.covers[house]
                    .structure
                    .as_ref()
                    .is_some_and(|state| state.standing(part))
            {
                sim.break_structure(
                    house,
                    PartSet::from_indices([part]),
                    PartSet::EMPTY,
                    None,
                    None,
                );
            }
        }
        hook_marks.lock().unwrap().left.push(Count::now());
    }));

    net.window = Some(Window::default());
    net.run(FIGHT_MS);
    let fight = net.window.replace(Window::default()).expect("measuring");
    {
        let sim = net.host.simulation.as_ref().expect("a round");
        let layout = &sim.covers[house]
            .structure
            .as_ref()
            .expect("a cottage")
            .layout;
        let parts: Vec<usize> = (0..layout.parts.len())
            .filter(|&i| {
                layout.parts[i].house().wall == Some(2)
                    || layout.parts[i].house().kind == PartKind::Post
            })
            .collect();
        *wall.lock().unwrap() = Some((net.host.tick() + 1, parts));
    }
    net.run(COLLAPSE_MS);
    let collapse = net.window.take().expect("measuring");
    let sim = net.host.simulation.as_ref().expect("a round");
    let falling = sim
        .fragments
        .iter()
        .filter(|fragment| fragment.of_structure(house))
        .count();
    let gone = sim.covers[house]
        .structure
        .as_ref()
        .map_or(0, |state| state.gone.len());
    let shown: Vec<usize> = net
        .peers
        .iter()
        .map(|peer| {
            peer.client
                .display()
                .map_or(0, |state| state.fragments.len())
        })
        .collect();
    println!(
        "village, host and two clients: cottage {} lost {gone} parts, {falling} of its bodies loose, {} fragments on the host, {shown:?} drawn, tick {}",
        sim.covers[house].id,
        sim.fragments.len(),
        net.host.tick()
    );
    println!(
        "hashes: sent {:016x}, drawn {:016x} {:016x}",
        net.sent.finish(),
        net.drawn[0].finish(),
        net.drawn[1].finish()
    );
    report("fight (2 s)", &fight);
    report("collapse (10 s)", &collapse);
}
