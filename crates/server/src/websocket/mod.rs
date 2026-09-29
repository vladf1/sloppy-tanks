//! RFC 6455 WebSocket framing with RFC 7692 permessage-deflate.
//!
//! No maintained Rust WebSocket crate offered what the former Node server's `ws` did here:
//! tungstenite 0.30 rejects every frame with RSV1 set (no permessage-deflate at all),
//! soketto's deflate extension builds a fresh compressor per message (no context
//! takeover, which is most of the saving on repetitive snapshots) and inflates without
//! an output bound, and yawc drags in rustls, reqwest-style dependencies and MPL
//! licensing. The protocol a server needs is small, so this module implements it over
//! `flate2` (zlib-rs backend, zlib level 1), matching `ws`'s behaviour: context takeover
//! in both directions unless the client asks otherwise, messages under 1 KiB sent
//! uncompressed, and one bounded message size for compressed and plain messages.

pub mod codec;
pub mod deflate;
pub mod extension;

pub use codec::{Codec, Event, ProtocolError, Role};
pub use extension::DeflateParams;

/// The GUID RFC 6455 appends to the client's key.
const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// `Sec-WebSocket-Accept` for a client's `Sec-WebSocket-Key`.
pub fn accept_key(key: &str) -> String {
    let mut hash = sha1_smol::Sha1::new();
    hash.update(key.as_bytes());
    hash.update(ACCEPT_GUID.as_bytes());
    base64(&hash.digest().bytes())
}

/// A key is 16 random bytes in base64 (`ws` checks `/^[+/0-9A-Za-z]{22}==$/`).
pub fn is_valid_key(key: &str) -> bool {
    key.len() == 24
        && key.ends_with("==")
        && key[..22]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
}

pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(ALPHABET[(triple >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// Close codes a peer may send (`ws`'s `isValidStatusCode`).
pub fn is_valid_close_code(code: u16) -> bool {
    (1000..=1014).contains(&code) && !matches!(code, 1004..=1006) || (3000..=4999).contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_key_matches_the_rfc_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert!(is_valid_key("dGhlIHNhbXBsZSBub25jZQ=="));
        assert!(!is_valid_key("dGhlIHNhbXBsZSBub25jZQ="));
        assert!(!is_valid_key("dGhlIHNhbXBsZSBub25jZ-=="));
    }

    #[test]
    fn base64_pads_short_input() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }

    #[test]
    fn close_codes_exclude_reserved_values() {
        assert!(is_valid_close_code(1000));
        assert!(is_valid_close_code(1012));
        assert!(is_valid_close_code(4002));
        assert!(!is_valid_close_code(1005));
        assert!(!is_valid_close_code(1006));
        assert!(!is_valid_close_code(999));
        assert!(!is_valid_close_code(2000));
    }
}
