//! The real room host: [`sloppy_core::net::match_host::MatchHost`] behind the server's
//! [`RoomHost`] contract.
//!
//! The core host is transport-free and queues its sends, closes and directory changes;
//! this adapter forwards them, in order, into the session's [`HostOutput`] after each call.

use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};

use crate::host::{ConnectionId, HostOptions, HostOutput, RoomHost};
use crate::room_list::RoomListing;

pub struct MatchRoom {
    host: MatchHost,
}

impl MatchRoom {
    pub fn new(options: HostOptions) -> Self {
        Self {
            host: MatchHost::new(MatchHostOptions {
                room_epoch: options.room_epoch,
                now_ms: options.now_ms,
                token: options.token,
                seed: Some(options.seed),
                content_version: Some(options.content_version),
            }),
        }
    }

    /// The core host, for tests and diagnostics.
    pub fn host(&self) -> &MatchHost {
        &self.host
    }

    fn flush(&mut self, out: &mut HostOutput) {
        for event in self.host.take_events() {
            match event {
                HostEvent::Send { connection, text } => out.send(ConnectionId(connection), text),
                HostEvent::Close {
                    connection,
                    code,
                    reason,
                } => out.close(ConnectionId(connection), code, reason),
                HostEvent::Changed => out.changed(),
            }
        }
    }
}

impl RoomHost for MatchRoom {
    fn receive(&mut self, connection: ConnectionId, text: &str, now_ms: u64, out: &mut HostOutput) {
        self.host.receive(connection.0, text, now_ms);
        self.flush(out);
    }

    fn disconnect(&mut self, connection: ConnectionId, now_ms: u64, out: &mut HostOutput) {
        self.host.disconnect(connection.0, now_ms);
        self.flush(out);
    }

    fn advance(&mut self, now_ms: u64, out: &mut HostOutput) {
        self.host.advance(now_ms);
        self.flush(out);
    }

    fn dispose(&mut self, reason: &str, out: &mut HostOutput) {
        self.host.dispose(reason);
        self.flush(out);
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
}
