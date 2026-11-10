//! The real room host: [`sloppy_core::net::match_host::MatchHost`] behind the server's
//! [`RoomHost`] contract.
//!
//! The core host is transport-free and queues its sends, closes and directory changes;
//! this adapter moves them, in order, into the session's [`HostOutput`] after each call.

use std::time::Instant;

use sloppy_core::net::match_host::{ConnectionId, MatchHost, MatchHostOptions};
use sloppy_core::net::room_list::RoomListing;

use crate::host::{HostOutput, RoomHost};

pub struct MatchRoom {
    host: MatchHost,
}

impl MatchRoom {
    pub fn new(options: MatchHostOptions) -> Self {
        Self {
            host: MatchHost::new(options),
        }
    }
}

impl RoomHost for MatchRoom {
    fn receive(&mut self, connection: ConnectionId, text: &str, now_ms: u64, out: &mut HostOutput) {
        let round = self.host.round_id;
        let received = Instant::now();
        self.host.receive(connection, text, now_ms);
        if self.host.round_id != round {
            // The message began a round, whose simulation was built before this returned:
            // tens of milliseconds on the large maps, which the round's clock must not
            // count as ticks it owes.
            let build_ms = received.elapsed().as_millis() as u64;
            self.host.round_built(now_ms + build_ms);
        }
        out.extend(self.host.drain_output());
    }

    fn disconnect(&mut self, connection: ConnectionId, now_ms: u64, out: &mut HostOutput) {
        self.host.disconnect(connection, now_ms);
        out.extend(self.host.drain_output());
    }

    fn advance(&mut self, now_ms: u64, out: &mut HostOutput) {
        self.host.advance(now_ms);
        out.extend(self.host.drain_output());
    }

    fn dispose(&mut self, reason: &str, out: &mut HostOutput) {
        self.host.dispose(reason);
        out.extend(self.host.drain_output());
    }

    fn is_disposed(&self) -> bool {
        self.host.disposed
    }

    fn dispose_reason(&self) -> Option<&str> {
        self.host.dispose_reason.as_deref()
    }

    fn directory_entry(&self, room: &str) -> RoomListing {
        self.host.directory_entry(room)
    }

    fn connections(&self) -> u32 {
        self.host.connections() as u32
    }

    fn tick(&self) -> u64 {
        self.host.tick()
    }

    fn debt_ms(&self) -> f64 {
        self.host.debt_ms()
    }

    fn until_next_batch_ms(&self, now_ms: u64) -> Option<f64> {
        self.host.until_next_batch_ms(now_ms)
    }

    fn input_lapses(&self) -> u64 {
        self.host.input_lapses()
    }
}
