//! A room for the pilot tests: the real host and pilots wired in memory with one clock,
//! as `crates/core/tests/integration/net_client.rs` wires browser clients. Sockets open a
//! frame after they are asked for, pilots tick at the Worker's frame rate and the host's
//! timer fires as the server's does. Import with `use crate::support;`.

use std::collections::BTreeMap;

use sloppy_core::net::client::ClientAction;
use sloppy_core::net::fixed_step_clock::HOST_INTERVAL_MS;
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{Message, RoomSettings};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Vec2, distance};
use sloppy_core::sim::render_state::RenderTank;
use sloppy_pilot::orders::{Mission, Orders};
use sloppy_pilot::pilot::{Pilot, PilotConfig};

/// The bots Worker ticks its pilots at about 30 Hz.
pub const FRAME_MS: f64 = 1000.0 / 30.0;
pub const ROOM: &str = "ABCD2345";

pub struct Seat {
    pub pilot: Pilot,
    /// The open socket: the pilot's id for it and the host connection behind it.
    socket: Option<(u32, u64)>,
    /// Sockets asked for and not yet reported open.
    opening: Vec<(u32, u64)>,
}

pub struct Room {
    pub host: MatchHost,
    pub now: f64,
    pub seats: Vec<Seat>,
    next_connection: u64,
    /// Host connection → (seat, socket).
    routes: BTreeMap<u64, (usize, u32)>,
    next_host_tick: f64,
}

/// Room settings for a five-minute battle with fill bots.
pub fn settings(map: MapId) -> RoomSettings {
    RoomSettings {
        map_mode: map,
        difficulty: Difficulty::Normal,
        humans_only: false,
        round_minutes: 5,
    }
}

pub fn orders(mission: Mission) -> Orders {
    Orders {
        mission,
        ..Orders::default()
    }
}

impl Room {
    pub fn new(seed: u32) -> Self {
        let mut token = 0;
        Self {
            host: MatchHost::new(MatchHostOptions {
                room_epoch: "pilot-room".into(),
                now_ms: 0,
                token: Box::new(move || {
                    token += 1;
                    format!("credential-{token:020}")
                }),
                seed,
                content_version: None,
            }),
            now: 0.0,
            seats: Vec::new(),
            next_connection: 0,
            routes: BTreeMap::new(),
            next_host_tick: HOST_INTERVAL_MS.ceil(),
        }
    }

    /// Seats a pilot named `name`, which starts connecting at once; `create` makes the
    /// room. Returns its seat index.
    pub fn add(&mut self, name: &str, orders: Orders, create: Option<RoomSettings>) -> usize {
        let mut pilot = Pilot::new(PilotConfig {
            server_url: "ws://127.0.0.1:8787".into(),
            room: ROOM.into(),
            name: name.into(),
            orders,
            create,
            saved_seat: None,
            seed: 11 + self.seats.len() as u32,
        });
        pilot.start(self.now);
        self.seats.push(Seat {
            pilot,
            socket: None,
            opening: Vec::new(),
        });
        self.settle();
        self.seats.len() - 1
    }

    /// Runs every clock for `ms`.
    pub fn run(&mut self, ms: f64) {
        let end = self.now + ms;
        while self.now < end {
            self.now += FRAME_MS;
            let now = self.now;
            for index in 0..self.seats.len() {
                for (socket, connection) in std::mem::take(&mut self.seats[index].opening) {
                    self.seats[index].socket = Some((socket, connection));
                    self.routes.insert(connection, (index, socket));
                    self.seats[index].pilot.socket_opened(socket, now);
                }
                self.seats[index].pilot.tick(now);
            }
            self.settle();
            while self.next_host_tick <= self.now {
                let at = self.next_host_tick as u64;
                self.host.advance(at);
                let wait = self
                    .host
                    .until_next_batch_ms(at)
                    .map_or(HOST_INTERVAL_MS, |wait| wait.max(1.0));
                self.next_host_tick += wait.ceil();
                self.settle();
            }
        }
    }

    /// Runs until `done` holds or `ms` pass; whether it held.
    pub fn run_until(&mut self, ms: f64, mut done: impl FnMut(&Room) -> bool) -> bool {
        let end = self.now + ms;
        while self.now < end {
            if done(self) {
                return true;
            }
            self.run(FRAME_MS);
        }
        done(self)
    }

    /// Performs pilot actions and delivers host messages until both are quiet.
    fn settle(&mut self) {
        loop {
            let mut busy = false;
            for index in 0..self.seats.len() {
                let actions = self.seats[index].pilot.take_actions();
                busy |= !actions.is_empty();
                for action in actions {
                    self.perform(index, action);
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
            ClientAction::OpenSocket { socket, url } => {
                assert_eq!(url, format!("ws://127.0.0.1:8787/room/{ROOM}"));
                self.next_connection += 1;
                self.seats[index]
                    .opening
                    .push((socket, self.next_connection));
            }
            ClientAction::Send { socket, text } => {
                if let Some((current, connection)) = self.seats[index].socket
                    && current == socket
                {
                    self.host.receive(connection, &text, now as u64);
                }
            }
            ClientAction::CloseSocket { socket } => {
                if let Some((current, connection)) = self.seats[index].socket
                    && current == socket
                {
                    self.seats[index].socket = None;
                    self.routes.remove(&connection);
                    self.host.disconnect(connection, now as u64);
                    self.seats[index].pilot.socket_closed(socket, 1005, now);
                }
                self.seats[index].opening.retain(|(id, _)| *id != socket);
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
                if let Some(&(index, socket)) = self.routes.get(&connection) {
                    let pilot = &mut self.seats[index].pilot;
                    match message {
                        Message::Text(text) => pilot.socket_text(socket, &text, now),
                        Message::Binary(bytes) => pilot.socket_binary(socket, &bytes, now),
                    }
                }
            }
            HostEvent::Close {
                connection, code, ..
            } => {
                if let Some((index, socket)) = self.routes.remove(&connection) {
                    self.seats[index].socket = None;
                    self.host.disconnect(connection, now as u64);
                    self.seats[index].pilot.socket_closed(socket, code, now);
                }
            }
            HostEvent::Changed => {}
        }
    }

    /// The tank a seat's pilot displays as its own.
    pub fn own_tank(&self, index: usize) -> Option<&RenderTank> {
        self.seats[index].pilot.scene()?.viewer()
    }

    /// Where the host has the tank of the player named `name`.
    pub fn host_tank(&self, name: &str) -> Option<(Vec2, bool, u8)> {
        let simulation = self.host.simulation.as_ref()?;
        let tank = simulation.tanks.iter().find(|tank| tank.name == name)?;
        let position = if tank.alive {
            simulation.tank_planar(simulation.tank_index(tank.id)?)
        } else {
            tank.previous
        };
        Some((position, tank.alive, tank.team as u8))
    }

    /// The planar distance between two players' tanks on the host, while both live.
    pub fn host_distance(&self, a: &str, b: &str) -> Option<f64> {
        let (a, a_alive, _) = self.host_tank(a)?;
        let (b, b_alive, _) = self.host_tank(b)?;
        (a_alive && b_alive).then(|| distance(a, b))
    }
}
