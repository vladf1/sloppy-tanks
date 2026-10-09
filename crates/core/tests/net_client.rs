//! The browser session state machine against the real host, wired in memory with one
//! clock: joining, arena preparation, baselines, input cadence and acks, pause and resume,
//! reconnects that keep the seat, and every way a connection ends.

use std::collections::BTreeMap;

use sloppy_core::net::client::{
    ClientAction, ClientConfig, ClientNotice, EndCause, LocalInput, NetworkClient, SavedSeat,
};
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{JoinChoice, Message, RoomPhase, RoomSettings};
use sloppy_core::net::transport_delay::DelaySettings;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{Driver, VehicleKind};

const FRAME_MS: f64 = 1000.0 / 60.0;

struct Peer {
    client: NetworkClient,
    /// The open socket: its id and the host connection behind it.
    socket: Option<(u32, u64)>,
    notices: Vec<ClientNotice>,
    saved: Option<SavedSeat>,
    /// Sockets the page opened but has not yet reported open.
    opening: Vec<(u32, u64)>,
    input: LocalInput,
    frames: usize,
}

struct Network {
    host: MatchHost,
    peers: Vec<Peer>,
    now: f64,
    next_connection: u64,
    /// Host connection → (peer, socket).
    routes: BTreeMap<u64, (usize, u32)>,
    next_host_tick: f64,
    /// Until then the host's messages queue up, as behind a TCP resend, and then arrive
    /// together.
    stalled_until: f64,
    held: Vec<HostEvent>,
}

fn choice(name: &str, create: Option<RoomSettings>) -> JoinChoice {
    JoinChoice {
        name: name.into(),
        kind: VehicleKind::Balanced,
        team: None,
        create,
        existing_room: None,
    }
}

fn settings(map: MapId) -> RoomSettings {
    RoomSettings {
        map_mode: map,
        difficulty: Difficulty::Normal,
        humans_only: false,
        round_minutes: 5,
    }
}

impl Network {
    fn new() -> Self {
        let mut token = 0;
        Self {
            host: MatchHost::new(MatchHostOptions {
                room_epoch: "epoch-1".into(),
                now_ms: 0,
                token: Box::new(move || {
                    token += 1;
                    format!("credential-{token:020}")
                }),
                seed: Some(4242),
                content_version: None,
            }),
            peers: Vec::new(),
            now: 0.0,
            next_connection: 0,
            routes: BTreeMap::new(),
            next_host_tick: 50.0,
            stalled_until: 0.0,
            held: Vec::new(),
        }
    }

    fn add(&mut self, delay: Option<DelaySettings>) -> usize {
        self.peers.push(Peer {
            client: NetworkClient::new(ClientConfig {
                server_url: "ws://127.0.0.1:8787/".into(),
                room: "ABCDEFGH".into(),
                saved_seat: None,
                delay,
                seed: 7,
            }),
            socket: None,
            notices: Vec::new(),
            saved: None,
            opening: Vec::new(),
            input: LocalInput::default(),
            frames: 0,
        });
        self.peers.len() - 1
    }

    /// Performs every pending client action and host event until both are quiet.
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
                self.respond(index, &notices);
                self.peers[index].notices.extend(notices);
            }
            let mut events = self.host.take_events();
            busy |= !events.is_empty();
            if self.now < self.stalled_until {
                self.held.append(&mut events);
            } else {
                events.splice(0..0, std::mem::take(&mut self.held));
            }
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
            ClientAction::OpenSocket { socket, url } => {
                assert_eq!(url, "ws://127.0.0.1:8787/room/ABCDEFGH");
                self.next_connection += 1;
                self.peers[index]
                    .opening
                    .push((socket, self.next_connection));
            }
            ClientAction::Send { socket, text } => {
                let Some((current, connection)) = self.peers[index].socket else {
                    return;
                };
                if current == socket {
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
                self.peers[index].opening.retain(|(id, _)| *id != socket);
            }
            ClientAction::SaveSeat { token, room_epoch } => {
                self.peers[index].saved = Some(SavedSeat { token, room_epoch });
            }
            ClientAction::ForgetSeat => self.peers[index].saved = None,
        }
    }

    /// The page's renderer: prepares arenas and draws baselines at once.
    fn respond(&mut self, index: usize, notices: &[ClientNotice]) {
        let now = self.now;
        for notice in notices {
            match notice {
                ClientNotice::PrepareArena {
                    round_id,
                    room_epoch,
                } => {
                    assert!(self.peers[index].client.arena_state().is_some());
                    self.peers[index]
                        .client
                        .arena_prepared(*round_id, room_epoch, now);
                }
                ClientNotice::ShowBaseline { baseline } => {
                    assert!(self.peers[index].client.display().is_some());
                    self.peers[index].client.baseline_shown(*baseline, now);
                }
                _ => {}
            }
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
                        Message::Binary(bytes) => client.socket_binary(socket, &bytes, now),
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

    /// Runs the page and host clocks for `ms`: sockets open after one frame, pages poll and
    /// draw at 60 Hz, the host ticks every 50 ms.
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
                self.peers[index].client.poll(now);
                let input = self.peers[index].input;
                if self.peers[index].client.frame(now, &input).is_some() {
                    self.peers[index].frames += 1;
                }
                self.peers[index].input.mine = false;
            }
            self.settle();
            while self.next_host_tick <= self.now {
                self.host.advance(self.next_host_tick as u64);
                self.next_host_tick += 50.0;
                self.settle();
            }
        }
    }

    fn connect(&mut self, index: usize, choice: JoinChoice) {
        let now = self.now;
        self.peers[index].client.connect(choice, now);
        self.settle();
    }

    fn ended(&self, index: usize) -> Option<(EndCause, String)> {
        self.peers[index]
            .notices
            .iter()
            .rev()
            .find_map(|notice| match notice {
                ClientNotice::Ended { cause, text } => Some((*cause, text.clone())),
                _ => None,
            })
    }

    fn tank_driver(&self, index: usize) -> Driver {
        let control = self.peers[index].client.control().unwrap();
        let sim = self.host.simulation.as_ref().unwrap();
        sim.tanks[sim.tank_index(control.tank_id).unwrap()].driver
    }
}

#[test]
fn a_created_room_prepares_the_arena_then_drives_with_acknowledged_input() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Village))));
    net.run(200.0);
    let peer = &net.peers[alice];
    assert!(peer.client.connected);
    assert_eq!(peer.client.phase(), RoomPhase::Playing);
    assert!(peer.saved.is_some(), "the seat is kept for reloads");
    assert!(
        peer.notices
            .iter()
            .any(|notice| matches!(notice, ClientNotice::PrepareArena { round_id: 1, .. }))
    );
    assert!(
        peer.notices
            .iter()
            .any(|notice| matches!(notice, ClientNotice::Reveal)),
        "a created room reveals once its arena draws"
    );
    assert!(net.peers[alice].client.active_input());
    assert_eq!(net.tank_driver(alice), Driver::Human);
    let start = {
        let sim = net.host.simulation.as_ref().unwrap();
        let control = net.peers[alice].client.control().unwrap();
        sim.body_translation(sim.tanks[sim.tank_index(control.tank_id).unwrap()].body)
    };
    net.peers[alice].input = LocalInput {
        move_z: 1.0,
        fire: true,
        aim_angle: 0.3,
        ..LocalInput::default()
    };
    net.run(2000.0);
    let stats = net.peers[alice].client.stats(net.now);
    assert!(
        stats.input_seq > 30,
        "held input goes out at 20 Hz: {stats:?}"
    );
    assert!(stats.input_ack > 0, "the host applied the input");
    assert!(stats.received_updates > 30);
    assert!(net.peers[alice].frames > 100, "frames were drawn");
    let sim = net.host.simulation.as_ref().unwrap();
    assert!(sim.shots_fired > 0);
    let control = net.peers[alice].client.control().unwrap();
    let tank = &sim.tanks[sim.tank_index(control.tank_id).unwrap()];
    let moved = sim.body_translation(tank.body);
    assert!(
        (moved.x - start.x).hypot(moved.z - start.z) > 1.0,
        "the tank drove"
    );
    // The display follows the host's scene: same tanks, the viewer near its hull.
    let display = net.peers[alice].client.display().unwrap();
    assert_eq!(display.tanks.len(), sim.tanks.len());
    let viewer = display.viewer().unwrap();
    assert!((viewer.position.x - moved.x).hypot(viewer.position.z - moved.z) < 3.0);
    assert_eq!(viewer.aim, 0.3, "own aim shows at once");
}

#[test]
fn a_baseline_shows_until_the_next_frame_draws_the_timeline_with_the_own_aim() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Village))));
    net.run(500.0);
    let now = net.now;
    net.peers[alice].client.resume(now);
    net.settle();
    let client = &net.peers[alice].client;
    assert!(client.active_input(), "the fresh baseline was drawn");
    assert_eq!(
        client.display(),
        client.arena_state().as_ref(),
        "no frame has read the timeline since the baseline"
    );
    let input = LocalInput {
        aim_angle: 0.3,
        ..LocalInput::default()
    };
    let now = net.now + FRAME_MS;
    let shown = net.peers[alice]
        .client
        .frame(now, &input)
        .unwrap()
        .state
        .clone();
    assert_eq!(shown.viewer().unwrap().aim, 0.3, "own aim shows at once");
    assert_eq!(net.peers[alice].client.display(), Some(&shown));
}

#[test]
fn late_snapshot_batches_count_only_while_the_stream_should_flow() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Harbor))));
    net.run(1000.0);
    let stats = net.peers[alice].client.stats(net.now);
    // Batches leave every 50 ms and the page reads them on its next 60 Hz frame.
    assert_eq!(stats.late_batches, 0, "{stats:?}");
    assert!(stats.longest_batch_gap_ms < 70.0, "{stats:?}");
    // A lost packet holds up the stream for 200 ms; then everything arrives at once.
    net.stalled_until = net.now + 200.0;
    net.run(1000.0);
    let stats = net.peers[alice].client.stats(net.now);
    assert_eq!(stats.late_batches, 1, "{stats:?}");
    assert!(
        (200.0..270.0).contains(&stats.longest_batch_gap_ms),
        "{stats:?}"
    );
    // A hidden page and the menu change what is sent, not how late it is.
    let now = net.now;
    net.peers[alice].client.set_hidden(true, now);
    net.run(2000.0);
    let now = net.now;
    net.peers[alice].client.set_hidden(false, now);
    net.run(1000.0);
    let now = net.now;
    net.peers[alice].client.pause(now);
    net.run(1000.0);
    assert_eq!(net.peers[alice].client.stats(net.now).late_batches, 1);
}

#[test]
fn pause_hands_the_tank_to_a_bot_and_resume_takes_it_back_with_a_fresh_baseline() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Harbor))));
    net.run(300.0);
    let now = net.now;
    net.peers[alice].client.pause(now);
    net.settle();
    assert!(net.peers[alice].client.menu_open());
    assert!(!net.peers[alice].client.active_input());
    net.run(300.0);
    assert_eq!(net.tank_driver(alice), Driver::Bot);
    let baselines = |net: &Network| {
        net.peers[alice]
            .notices
            .iter()
            .filter(|notice| matches!(notice, ClientNotice::ShowBaseline { .. }))
            .count()
    };
    let before = baselines(&net);
    let now = net.now;
    net.peers[alice].client.resume(now);
    net.run(300.0);
    assert_eq!(net.tank_driver(alice), Driver::Human);
    assert!(baselines(&net) > before, "resume draws a fresh baseline");
    assert!(net.peers[alice].client.active_input());
}

#[test]
fn the_battle_keeps_playing_behind_the_menu_and_a_hidden_page_stops_the_stream() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Harbor))));
    net.run(300.0);
    let shown_time = |net: &Network| net.peers[alice].client.display().unwrap().match_state.time;
    let now = net.now;
    net.peers[alice].client.pause(now);
    let (time, frames) = (shown_time(&net), net.peers[alice].frames);
    // Longer than the host waits for an unacknowledged stream: pings acknowledge it.
    net.run(4000.0);
    assert!(net.peers[alice].client.menu_open());
    assert_eq!(net.tank_driver(alice), Driver::Bot);
    assert!(
        net.peers[alice].client.connected,
        "the watching seat stays connected"
    );
    assert!(
        net.peers[alice].frames > frames + 200,
        "frames keep drawing"
    );
    assert!(
        time - shown_time(&net) > 3.0,
        "the shown match clock keeps running"
    );
    // A hidden page gets no stream; shown again behind the menu, it watches once more.
    let now = net.now;
    net.peers[alice].client.set_hidden(true, now);
    net.run(1000.0);
    let frames = net.peers[alice].frames;
    net.run(1000.0);
    assert_eq!(
        net.peers[alice].frames, frames,
        "a hidden page draws nothing"
    );
    let now = net.now;
    net.peers[alice].client.set_hidden(false, now);
    net.run(1000.0);
    let time = shown_time(&net);
    net.run(1000.0);
    assert!(
        time - shown_time(&net) > 0.5,
        "the battle plays behind the menu again"
    );
    assert!(net.peers[alice].client.menu_open());
    assert_eq!(net.tank_driver(alice), Driver::Bot);
    // Hidden and shown again before a snapshot goes missing: the display still resumes.
    let now = net.now;
    net.peers[alice].client.set_hidden(true, now);
    net.peers[alice].client.set_hidden(false, now);
    net.run(1000.0);
    let time = shown_time(&net);
    net.run(1000.0);
    assert!(
        time - shown_time(&net) > 0.5,
        "a quick hide and show still plays behind the menu"
    );
}

#[test]
fn a_dropped_socket_reconnects_within_the_grace_and_keeps_its_seat() {
    let mut net = Network::new();
    let alice = net.add(None);
    let bob = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Quarry))));
    net.connect(bob, choice("bob", None));
    net.run(300.0);
    let player = net.peers[bob].client.player_id.clone();
    let tank = net.peers[bob].client.control().unwrap().tank_id;
    // The network drops bob's socket without a close handshake.
    let (socket, connection) = net.peers[bob].socket.take().unwrap();
    net.routes.remove(&connection);
    let now = net.now;
    net.host.disconnect(connection, now as u64);
    net.peers[bob].client.socket_closed(socket, 1006, now);
    net.settle();
    assert!(!net.peers[bob].client.connected);
    assert!(net.peers[bob].notices.iter().any(|notice| matches!(
        notice,
        ClientNotice::Status { text, connected: false } if text == "Reconnecting…"
    )));
    net.run(1500.0);
    let client = &net.peers[bob].client;
    assert!(client.connected, "the backoff retried");
    assert_eq!(client.player_id, player, "the seat was kept");
    assert_eq!(client.control().unwrap().tank_id, tank);
    assert_eq!(net.host.connections(), 2);
    assert_eq!(net.tank_driver(bob), Driver::Human);
    assert!(net.peers[bob].client.active_input());
}

#[test]
fn a_page_reload_rejoins_its_saved_seat() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Village))));
    net.run(200.0);
    let saved = net.peers[alice].saved.clone().unwrap();
    let player = net.peers[alice].client.player_id.clone();
    // The old page goes away; its socket closes.
    let now = net.now;
    net.peers[alice].client.stop();
    net.settle();
    net.run(100.0);
    let reloaded = net.add(None);
    net.peers[reloaded].client = NetworkClient::new(ClientConfig {
        server_url: "ws://127.0.0.1:8787".into(),
        room: "ABCDEFGH".into(),
        saved_seat: Some(saved),
        delay: None,
        seed: 1,
    });
    let _ = now;
    net.connect(
        reloaded,
        JoinChoice {
            existing_room: Some(true),
            ..choice("alice", None)
        },
    );
    net.run(300.0);
    assert_eq!(net.peers[reloaded].client.player_id, player);
    assert!(net.peers[reloaded].client.active_input());
}

#[test]
fn heartbeats_ping_every_second_and_measure_round_trips() {
    let mut net = Network::new();
    let alice = net.add(Some(DelaySettings {
        half_ms: 40.0,
        jitter_ms: 0.0,
        stall_ms: 0.0,
    }));
    net.connect(alice, choice("alice", Some(settings(MapId::Village))));
    net.run(3000.0);
    let stats = net.peers[alice].client.stats(net.now);
    assert!(net.peers[alice].client.connected);
    assert!(
        stats.rtt_ms >= 80.0 && stats.rtt_ms < 120.0,
        "a 40 ms each way delay measures ~80 ms: {}",
        stats.rtt_ms
    );
    assert!(net.peers[alice].client.active_input());
}

#[test]
fn a_room_reset_ends_the_connection_and_forgets_the_seat() {
    let mut net = Network::new();
    let alice = net.add(None);
    net.connect(alice, choice("alice", Some(settings(MapId::Village))));
    net.run(200.0);
    net.host.dispose("server-restart");
    net.settle();
    assert_eq!(
        net.ended(alice),
        Some((
            EndCause::RoomEnded,
            "The game server restarted, which closed every room.".into()
        ))
    );
    assert!(net.peers[alice].saved.is_none());
    assert!(net.peers[alice].client.stopped);
}

#[test]
fn a_second_tab_takes_the_seat_and_the_first_ends_as_other_tab() {
    let mut net = Network::new();
    let first = net.add(None);
    net.connect(first, choice("alice", Some(settings(MapId::Village))));
    net.run(200.0);
    let saved = net.peers[first].saved.clone().unwrap();
    let second = net.add(None);
    net.peers[second].client = NetworkClient::new(ClientConfig {
        server_url: "ws://127.0.0.1:8787".into(),
        room: "ABCDEFGH".into(),
        saved_seat: Some(saved),
        delay: None,
        seed: 1,
    });
    net.connect(second, choice("alice", None));
    net.run(200.0);
    assert_eq!(
        net.ended(first).map(|(cause, _)| cause),
        Some(EndCause::OtherTab)
    );
    assert!(net.peers[second].client.connected);
}

#[test]
fn an_expired_seat_or_a_gone_room_ends_with_its_own_cause() {
    let mut net = Network::new();
    let stale = net.add(None);
    net.peers[stale].client = NetworkClient::new(ClientConfig {
        server_url: "ws://127.0.0.1:8787".into(),
        room: "ABCDEFGH".into(),
        saved_seat: Some(SavedSeat {
            token: "credential-never-issued".into(),
            room_epoch: "epoch-1".into(),
        }),
        delay: None,
        seed: 1,
    });
    net.connect(stale, choice("zed", None));
    net.run(100.0);
    assert_eq!(
        net.ended(stale).map(|(cause, _)| cause),
        Some(EndCause::SeatExpired)
    );
    let gone = net.add(None);
    net.connect(
        gone,
        JoinChoice {
            existing_room: Some(true),
            ..choice("late", None)
        },
    );
    net.run(100.0);
    assert_eq!(
        net.ended(gone),
        Some((EndCause::RoomEnded, "This room has ended.".into()))
    );
}

#[test]
fn a_server_that_never_answers_ends_as_lost_after_the_reconnect_window() {
    let mut client = NetworkClient::new(ClientConfig {
        server_url: "ws://127.0.0.1:1".into(),
        room: "ABCDEFGH".into(),
        saved_seat: None,
        delay: None,
        seed: 1,
    });
    let mut now = 0.0;
    client.connect(choice("alice", None), now);
    let mut opened = 0;
    let mut lost = None;
    while now < 60_000.0 && lost.is_none() {
        for action in client.take_actions() {
            if let ClientAction::OpenSocket { socket, .. } = action {
                opened += 1;
                // Every attempt fails before the socket opens.
                client.socket_closed(socket, 1006, now);
            }
        }
        for notice in client.take_notices() {
            if let ClientNotice::Ended { cause, .. } = notice {
                lost = Some((cause, now));
            }
        }
        now += 100.0;
        client.poll(now);
    }
    let (cause, at) = lost.expect("the client gives up");
    assert_eq!(cause, EndCause::Lost);
    assert!((30_000.0..36_000.0).contains(&at), "gave up at {at}");
    assert!(opened >= 6, "backoff retried {opened} times");
}
