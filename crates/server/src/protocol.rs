//! Protocol constants the server layer shares with clients and with the room host.
//!
//! The shared values live in `sloppy_core::net::protocol` (the port of
//! `src/net/protocol.ts`); this module re-exports them beside the server-only build stamp.

pub use sloppy_core::net::protocol::{
    CONTENT_VERSION, EMPTY_GRACE_MS, MAX_BATTLE_OVERRUN_MS, MAX_CLIENT_MESSAGE_BYTES, MAX_ROOM_MS,
    Message, PROTOCOL_VERSION, ROOM_IDLE_MS, is_room_code,
};

/// Fingerprint of everything built into this server, so a deploy check can tell
/// server-only changes apart from the client-facing [`CONTENT_VERSION`].
pub const SERVER_BUILD: &str = match option_env!("SLOPPY_SERVER_BUILD") {
    Some(build) => build,
    None => "dev",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_codes_are_eight_unambiguous_characters() {
        assert!(is_room_code("ABCDEFGH"));
        assert!(is_room_code("TESTR2M9"));
        assert!(!is_room_code("abcdefgh"));
        assert!(!is_room_code("ABCDEFG"));
        assert!(!is_room_code("ABCDEFGHJ"));
        assert!(!is_room_code("ABCDEF01"), "0 and 1 are not in the alphabet");
        assert!(!is_room_code("ABCDÉFGH"));
    }
}
