//! Per-message compression state for one connection (RFC 7692 section 7.2).

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress};

use super::codec::{INVALID_COMPRESSED, ProtocolError, Role, TOO_LARGE};
use super::extension::DeflateParams;

/// Consecutive snapshot batches repeat each other's structure and small differences, so
/// permessage-deflate still sends about a third less of the binary state. zlib-rs level 1
/// (zlib-ng's quick strategy, static Huffman codes only) sends 4-12% more of it for less
/// than half the CPU; level 2, the fast strategy, is where the curve bends: levels 4 and 6 save
/// only 1-3% more bytes for 12-25% more time. Bytes count against the host's monthly
/// transfer, while deflate is a small share of a room's 50 ms interval next to the
/// simulation.
pub const COMPRESSION_LEVEL: u32 = 2;
/// Text messages shorter than this go out uncompressed, like `ws`'s default threshold:
/// the deflate block overhead outweighs the saving on pongs and control messages.
pub const COMPRESSION_THRESHOLD: usize = 1024;
/// Binary state messages (snapshot batches and baselines) shorter than this go out
/// uncompressed. Batches are a few hundred bytes, but consecutive ones repeat each other,
/// so with context takeover even small ones shrink.
pub const BINARY_COMPRESSION_THRESHOLD: usize = 64;
/// The empty stored block a sync flush ends with; senders strip it and receivers add it.
const SYNC_TAIL: [u8; 4] = [0x00, 0x00, 0xff, 0xff];
const DEFAULT_WINDOW_BITS: u8 = 15;

pub struct Deflate {
    compress: Compress,
    decompress: Decompress,
    reset_compressor: bool,
    reset_decompressor: bool,
}

impl Deflate {
    /// Compression state for one side of a connection with the negotiated parameters.
    pub fn new(params: &DeflateParams, role: Role) -> Self {
        let (own_bits, own_reset, peer_reset) = match role {
            Role::Server => (
                params.server_max_window_bits,
                params.server_no_context_takeover,
                params.client_no_context_takeover,
            ),
            Role::Client => (
                params.client_max_window_bits.flatten(),
                params.client_no_context_takeover,
                params.server_no_context_takeover,
            ),
        };
        // zlib cannot make raw deflate with an 8-bit window; negotiation declines it.
        let compress_bits = own_bits.unwrap_or(DEFAULT_WINDOW_BITS).max(9);
        Self {
            compress: Compress::new_with_window_bits(
                Compression::new(COMPRESSION_LEVEL),
                false,
                compress_bits,
            ),
            // A 15-bit window inflates anything a peer compressed with a smaller one.
            decompress: Decompress::new_with_window_bits(false, DEFAULT_WINDOW_BITS),
            reset_compressor: own_reset,
            reset_decompressor: peer_reset,
        }
    }

    /// Compresses one message payload, without the trailing sync-flush block.
    pub fn compress(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len() / 2 + 64);
        let mut consumed = 0;
        loop {
            let before = self.compress.total_in();
            self.compress
                .compress_vec(&input[consumed..], &mut output, FlushCompress::Sync)
                .expect("deflate accepts any input");
            consumed += (self.compress.total_in() - before) as usize;
            // zlib finished the flush when it left output space unused.
            if consumed == input.len() && output.len() < output.capacity() {
                break;
            }
            output.reserve(output.capacity().max(1024));
        }
        debug_assert!(output.ends_with(&SYNC_TAIL));
        output.truncate(output.len().saturating_sub(SYNC_TAIL.len()));
        if self.reset_compressor {
            // Keeps the level and window, like a fresh compressor.
            self.compress.reset();
        }
        output
    }

    /// Inflates one message payload into at most `limit` bytes; a larger or corrupt
    /// message is a protocol error.
    pub fn decompress(&mut self, input: &[u8], limit: usize) -> Result<Vec<u8>, ProtocolError> {
        let mut output = Vec::with_capacity((input.len() * 4).clamp(256, limit + 1));
        let result = self
            .inflate(input, limit, &mut output)
            .and_then(|()| self.inflate(&SYNC_TAIL, limit, &mut output));
        if self.reset_decompressor {
            // Back to the default raw window, DEFAULT_WINDOW_BITS.
            self.decompress.reset(false);
        }
        result.map(|()| output)
    }

    fn inflate(
        &mut self,
        input: &[u8],
        limit: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), ProtocolError> {
        let mut consumed = 0;
        loop {
            let (before_in, before_out) = (self.decompress.total_in(), self.decompress.total_out());
            self.decompress
                .decompress_vec(&input[consumed..], output, FlushDecompress::Sync)
                .map_err(|_| INVALID_COMPRESSED)?;
            consumed += (self.decompress.total_in() - before_in) as usize;
            if output.len() > limit {
                return Err(TOO_LARGE);
            }
            if consumed == input.len() && output.len() < output.capacity() {
                return Ok(());
            }
            if output.len() == output.capacity() {
                // Room for one byte past the limit shows that the message is too large.
                let target = (output.capacity() * 2).min(limit + 1);
                if target <= output.capacity() {
                    return Err(TOO_LARGE);
                }
                output.reserve_exact(target - output.len());
            } else if self.decompress.total_in() == before_in
                && self.decompress.total_out() == before_out
            {
                return Err(INVALID_COMPRESSED);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(params: &DeflateParams) -> (Deflate, Deflate) {
        (
            Deflate::new(params, Role::Server),
            Deflate::new(params, Role::Client),
        )
    }

    fn snapshot(index: usize) -> String {
        format!(
            r#"{{"type":"snapshot","roundId":1,"ack":{index},"snapshots":[{{"tick":{index},"tanks":[{}]}}]}}"#,
            (0..30)
                .map(|tank| format!(
                    r#"{{"id":{tank},"x":{}.25,"z":-{}.5,"hull":100}}"#,
                    tank * 3,
                    tank + index
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    #[test]
    fn round_trips_messages_with_context_takeover() {
        let (mut server, mut client) = pair(&DeflateParams::default());
        let mut sizes = Vec::new();
        for index in 0..5 {
            let text = snapshot(index);
            let packed = server.compress(text.as_bytes());
            sizes.push(packed.len());
            assert!(packed.len() < text.len() / 2);
            assert_eq!(
                client.decompress(&packed, 1 << 20).unwrap(),
                text.as_bytes()
            );
        }
        assert!(
            sizes[4] < sizes[0],
            "later messages reuse the shared window: {sizes:?}"
        );
    }

    #[test]
    fn no_context_takeover_resets_between_messages() {
        let params = DeflateParams {
            server_no_context_takeover: true,
            ..DeflateParams::default()
        };
        let (mut server, _) = pair(&params);
        let text = snapshot(1);
        let first = server.compress(text.as_bytes());
        assert_eq!(server.compress(text.as_bytes()), first);
        // A fresh decoder (the peer honours no-context-takeover) reads each one.
        let mut fresh = Deflate::new(&params, Role::Client);
        assert_eq!(fresh.decompress(&first, 1 << 20).unwrap(), text.as_bytes());
    }

    #[test]
    fn inflating_stops_at_the_size_limit() {
        let (mut server, mut client) = pair(&DeflateParams::default());
        let bomb = server.compress(&vec![b'a'; 100_000]);
        assert!(bomb.len() < 1000);
        assert_eq!(client.decompress(&bomb, 8192).unwrap_err().code, 1009);
        let (mut server, mut client) = pair(&DeflateParams::default());
        let exact = server.compress(&vec![b'a'; 8192]);
        assert_eq!(client.decompress(&exact, 8192).unwrap().len(), 8192);
    }

    #[test]
    fn corrupt_input_is_an_error() {
        let (_, mut client) = pair(&DeflateParams::default());
        assert!(
            client
                .decompress(&[0xff, 0xff, 0xff, 0x00, 0x13], 8192)
                .is_err()
        );
    }

    #[test]
    fn limited_windows_still_round_trip() {
        let params = DeflateParams {
            server_max_window_bits: Some(9),
            client_max_window_bits: Some(Some(10)),
            ..DeflateParams::default()
        };
        let (mut server, mut client) = pair(&params);
        let text = snapshot(3);
        assert_eq!(
            client
                .decompress(&server.compress(text.as_bytes()), 1 << 20)
                .unwrap(),
            text.as_bytes()
        );
        assert_eq!(
            server
                .decompress(&client.compress(text.as_bytes()), 1 << 20)
                .unwrap(),
            text.as_bytes()
        );
    }
}
