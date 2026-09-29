//! Protocol constants the server layer shares with clients and with the room host.
//!
//! These mirror `src/net/protocol.ts`. When `sloppy_core::net` gains the protocol port,
//! the shared values should move there and this module should re-export them.

/// Wire protocol generation; `/health` reports it and joins must match it.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hash of the sources clients and server must agree on. The build stamps it through
/// `SLOPPY_CONTENT_VERSION`; unstamped builds (tests, `cargo run`) use `test-content`.
pub const CONTENT_VERSION: &str = match option_env!("SLOPPY_CONTENT_VERSION") {
    Some(version) => version,
    None => "test-content",
};

/// Fingerprint of everything built into this server, so a deploy check can tell
/// server-only changes apart from the client-facing [`CONTENT_VERSION`].
pub const SERVER_BUILD: &str = match option_env!("SLOPPY_SERVER_BUILD") {
    Some(build) => build,
    None => "dev",
};

/// Largest client message, in UTF-8 bytes, that a room accepts.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 4096;

/// A dropped connection keeps its seat, and an emptied room stays, this long.
pub const EMPTY_GRACE_MS: u64 = 30_000;
/// Lobbies and results screens without activity expire after this long.
pub const ROOM_IDLE_MS: u64 = 5 * 60_000;
pub const DEFAULT_ROUND_MINUTES: u32 = 20;
pub const MAX_ROUND_MINUTES: u32 = 99;
/// A room hosts no new battle after this long; one already under way may finish.
pub const MAX_ROOM_MS: u64 = 4 * 60 * 60_000;
/// How far a battle may run past [`MAX_ROOM_MS`]: its longest length plus overtime, so
/// an endless next-kill overtime still cannot hold a room open forever.
pub const MAX_BATTLE_OVERRUN_MS: u64 = (MAX_ROUND_MINUTES as u64 + 30) * 60_000;

/// Room codes are eight characters from an alphabet without look-alikes
/// (`/^[A-Z2-9]{8}$/` in the TypeScript).
pub fn is_room_code(code: &str) -> bool {
    code.len() == 8
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || (b'2'..=b'9').contains(&byte))
}

/// Maps listed only on `/rooms?extralevels` (`isExtraLevel` in `src/game/map-options.ts`).
pub const EXTRA_LEVEL_MAPS: [&str; 2] = ["stress-test", "superstress"];

pub fn is_extra_level(map_mode: &str) -> bool {
    EXTRA_LEVEL_MAPS.contains(&map_mode)
}

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
