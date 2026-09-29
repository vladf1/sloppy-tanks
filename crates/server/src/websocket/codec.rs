//! Frame parsing and message assembly (RFC 6455 section 5), independent of I/O.

use bytes::{Buf, BufMut, BytesMut};

use super::deflate::{COMPRESSION_THRESHOLD, Deflate, InflateError};
use super::extension::DeflateParams;
use super::is_valid_close_code;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Reads masked frames and writes unmasked ones.
    Server,
    /// Writes masked frames; used by tests and tools that play a browser.
    Client,
}

const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xa;
const MAX_CONTROL_PAYLOAD: usize = 125;
/// Longest close reason that fits a control frame after the two-byte code.
const MAX_CLOSE_REASON: usize = MAX_CONTROL_PAYLOAD - 2;

/// Something a peer sent.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    /// The peer's close frame: its code (`None` when it sent none) and reason.
    Close(Option<u16>, String),
}

/// The peer broke the protocol. `code` is the close code the failure maps to (1002
/// protocol error, 1007 invalid UTF-8, 1009 too large); like `ws`, the server then drops
/// the connection without a closing handshake.
#[derive(Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: u16,
    pub message: &'static str,
}

const fn protocol(message: &'static str) -> ProtocolError {
    ProtocolError {
        code: 1002,
        message,
    }
}

struct Partial {
    opcode: u8,
    compressed: bool,
    payload: Vec<u8>,
}

/// One side of a WebSocket connection's framing and compression state.
pub struct Codec {
    role: Role,
    deflate: Option<Deflate>,
    max_message_bytes: usize,
    partial: Option<Partial>,
}

impl Codec {
    /// `max_message_bytes` bounds every data message after reassembly and inflation.
    pub fn new(role: Role, deflate: Option<&DeflateParams>, max_message_bytes: usize) -> Self {
        Self {
            role,
            deflate: deflate.map(|params| Deflate::new(params, role)),
            max_message_bytes,
            partial: None,
        }
    }

    pub fn compresses(&self) -> bool {
        self.deflate.is_some()
    }

    /// Takes the next complete event out of `buffer`; `Ok(None)` when more bytes are needed.
    pub fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Event>, ProtocolError> {
        loop {
            let Some((header, payload)) = self.next_frame(buffer)? else {
                return Ok(None);
            };
            if let Some(event) = self.frame(header, payload)? {
                return Ok(Some(event));
            }
        }
    }

    fn next_frame(&self, buffer: &mut BytesMut) -> Result<Option<(u8, Vec<u8>)>, ProtocolError> {
        if buffer.len() < 2 {
            return Ok(None);
        }
        let (first, second) = (buffer[0], buffer[1]);
        let masked = second & 0x80 != 0;
        if masked != (self.role == Role::Server) {
            return Err(protocol(if masked {
                "Unexpected mask"
            } else {
                "Mask required"
            }));
        }
        let (length, mut offset) = match second & 0x7f {
            126 if buffer.len() >= 4 => (u64::from(u16::from_be_bytes([buffer[2], buffer[3]])), 4),
            127 if buffer.len() >= 10 => (
                u64::from_be_bytes(buffer[2..10].try_into().expect("eight bytes")),
                10,
            ),
            126 | 127 => return Ok(None),
            length => (u64::from(length), 2),
        };
        // Refuse before buffering: a frame can never be larger than a whole message.
        if length > self.max_message_bytes as u64 {
            return Err(ProtocolError {
                code: 1009,
                message: "Max payload size exceeded",
            });
        }
        let length = length as usize;
        let mask = if masked {
            if buffer.len() < offset + 4 {
                return Ok(None);
            }
            offset += 4;
            Some([
                buffer[offset - 4],
                buffer[offset - 3],
                buffer[offset - 2],
                buffer[offset - 1],
            ])
        } else {
            None
        };
        if buffer.len() < offset + length {
            buffer.reserve(offset + length - buffer.len());
            return Ok(None);
        }
        buffer.advance(offset);
        let mut payload = buffer.split_to(length).to_vec();
        if let Some(mask) = mask {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        Ok(Some((first, payload)))
    }

    fn frame(&mut self, header: u8, payload: Vec<u8>) -> Result<Option<Event>, ProtocolError> {
        let fin = header & 0x80 != 0;
        let rsv1 = header & 0x40 != 0;
        let opcode = header & 0x0f;
        if header & 0x30 != 0 {
            return Err(protocol("RSV2 and RSV3 must be clear"));
        }
        if opcode >= OP_CLOSE {
            if !fin || payload.len() > MAX_CONTROL_PAYLOAD || rsv1 {
                return Err(protocol("Invalid control frame"));
            }
            return match opcode {
                OP_CLOSE => close_event(&payload).map(Some),
                OP_PING => Ok(Some(Event::Ping(payload))),
                OP_PONG => Ok(Some(Event::Pong)),
                _ => Err(protocol("Invalid opcode")),
            };
        }
        let mut partial = match (opcode, self.partial.take()) {
            (OP_CONTINUATION, Some(partial)) if !rsv1 => partial,
            (OP_CONTINUATION, _) => return Err(protocol("Unexpected continuation frame")),
            (OP_TEXT | OP_BINARY, None) => {
                if rsv1 && self.deflate.is_none() {
                    return Err(protocol("RSV1 must be clear"));
                }
                Partial {
                    opcode,
                    compressed: rsv1,
                    payload: Vec::new(),
                }
            }
            (OP_TEXT | OP_BINARY, Some(_)) => {
                return Err(protocol("Expected a continuation frame"));
            }
            _ => return Err(protocol("Invalid opcode")),
        };
        if partial.payload.len() + payload.len() > self.max_message_bytes {
            return Err(ProtocolError {
                code: 1009,
                message: "Max payload size exceeded",
            });
        }
        partial.payload.extend_from_slice(&payload);
        if !fin {
            self.partial = Some(partial);
            return Ok(None);
        }
        let data = if partial.compressed {
            let deflate = self
                .deflate
                .as_mut()
                .expect("compressed frames need the extension");
            deflate
                .decompress(&partial.payload, self.max_message_bytes)
                .map_err(|error| match error {
                    InflateError::TooLarge => ProtocolError {
                        code: 1009,
                        message: "Max payload size exceeded",
                    },
                    InflateError::Corrupt => ProtocolError {
                        code: 1007,
                        message: "Invalid compressed data",
                    },
                })?
        } else {
            partial.payload
        };
        if partial.opcode == OP_BINARY {
            return Ok(Some(Event::Binary(data)));
        }
        String::from_utf8(data)
            .map(|text| Some(Event::Text(text)))
            .map_err(|_| ProtocolError {
                code: 1007,
                message: "Invalid UTF-8 sequence",
            })
    }

    /// Appends one text message as a single frame, compressed when negotiated and worth it.
    pub fn encode_text(&mut self, text: &str, output: &mut BytesMut) {
        match self.deflate.as_mut() {
            Some(deflate) if text.len() >= COMPRESSION_THRESHOLD => {
                let packed = deflate.compress(text.as_bytes());
                self.encode_frame(0x80 | 0x40 | OP_TEXT, &packed, output);
            }
            _ => self.encode_frame(0x80 | OP_TEXT, text.as_bytes(), output),
        }
    }

    pub fn encode_binary(&mut self, data: &[u8], output: &mut BytesMut) {
        self.encode_frame(0x80 | OP_BINARY, data, output);
    }

    pub fn encode_close(&mut self, code: Option<u16>, reason: &str, output: &mut BytesMut) {
        let mut payload = Vec::with_capacity(2 + reason.len());
        if let Some(code) = code {
            payload.extend_from_slice(&code.to_be_bytes());
            payload.extend_from_slice(truncate_utf8(reason, MAX_CLOSE_REASON).as_bytes());
        }
        self.encode_frame(0x80 | OP_CLOSE, &payload, output);
    }

    pub fn encode_ping(&mut self, payload: &[u8], output: &mut BytesMut) {
        self.encode_frame(
            0x80 | OP_PING,
            &payload[..payload.len().min(MAX_CONTROL_PAYLOAD)],
            output,
        );
    }

    pub fn encode_pong(&mut self, payload: &[u8], output: &mut BytesMut) {
        self.encode_frame(
            0x80 | OP_PONG,
            &payload[..payload.len().min(MAX_CONTROL_PAYLOAD)],
            output,
        );
    }

    fn encode_frame(&mut self, header: u8, payload: &[u8], output: &mut BytesMut) {
        let mask_bit = if self.role == Role::Client { 0x80 } else { 0 };
        output.reserve(payload.len() + 14);
        output.put_u8(header);
        match payload.len() {
            length @ 0..=125 => output.put_u8(mask_bit | length as u8),
            length @ 126..=0xffff => {
                output.put_u8(mask_bit | 126);
                output.put_u16(length as u16);
            }
            length => {
                output.put_u8(mask_bit | 127);
                output.put_u64(length as u64);
            }
        }
        if self.role == Role::Client {
            let mut mask = [0u8; 4];
            getrandom::fill(&mut mask).expect("operating system randomness");
            output.put_slice(&mask);
            output.extend(
                payload
                    .iter()
                    .enumerate()
                    .map(|(index, byte)| byte ^ mask[index % 4]),
            );
        } else {
            output.put_slice(payload);
        }
    }
}

fn close_event(payload: &[u8]) -> Result<Event, ProtocolError> {
    match payload.len() {
        0 => Ok(Event::Close(None, String::new())),
        1 => Err(protocol("Invalid close frame")),
        _ => {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            if !is_valid_close_code(code) {
                return Err(protocol("Invalid close code"));
            }
            let reason = std::str::from_utf8(&payload[2..]).map_err(|_| ProtocolError {
                code: 1007,
                message: "Invalid UTF-8 sequence",
            })?;
            Ok(Event::Close(Some(code), reason.to_string()))
        }
    }
}

fn truncate_utf8(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(deflate: Option<&DeflateParams>) -> (Codec, Codec) {
        (
            Codec::new(Role::Server, deflate, 8192),
            Codec::new(Role::Client, deflate, 1 << 20),
        )
    }

    fn decode_all(codec: &mut Codec, bytes: &[u8]) -> Result<Vec<Event>, ProtocolError> {
        let mut buffer = BytesMut::from(bytes);
        let mut events = Vec::new();
        while let Some(event) = codec.decode(&mut buffer)? {
            events.push(event);
        }
        Ok(events)
    }

    #[test]
    fn text_round_trips_both_ways_with_and_without_deflate() {
        let params = DeflateParams::default();
        for deflate in [None, Some(&params)] {
            let (mut server, mut client) = pair(deflate);
            for text in ["hi".to_string(), "é".repeat(700), "x".repeat(5000)] {
                let mut wire = BytesMut::new();
                client.encode_text(&text, &mut wire);
                assert_eq!(
                    decode_all(&mut server, &wire).unwrap(),
                    [Event::Text(text.clone())]
                );
                let mut wire = BytesMut::new();
                server.encode_text(&text, &mut wire);
                assert_eq!(
                    decode_all(&mut client, &wire).unwrap(),
                    [Event::Text(text.clone())]
                );
            }
        }
    }

    #[test]
    fn small_messages_stay_uncompressed_and_large_ones_set_rsv1() {
        let params = DeflateParams::default();
        let (mut server, _) = pair(Some(&params));
        let mut wire = BytesMut::new();
        server.encode_text(r#"{"type":"pong","t":1,"tick":0}"#, &mut wire);
        assert_eq!(wire[0], 0x81);
        let mut wire = BytesMut::new();
        server.encode_text(&"{\"type\":\"snapshot\"}".repeat(100), &mut wire);
        assert_eq!(wire[0], 0xc1);
        assert!(wire.len() < 200);
    }

    #[test]
    fn decodes_byte_by_byte_and_reassembles_fragments() {
        let (mut server, mut client) = pair(None);
        let mut wire = BytesMut::new();
        client.encode_frame(OP_TEXT, b"hel", &mut wire);
        client.encode_ping(b"p", &mut wire);
        client.encode_frame(0x80 | OP_CONTINUATION, b"lo", &mut wire);
        let mut buffer = BytesMut::new();
        let mut events = Vec::new();
        for byte in wire.iter() {
            buffer.put_u8(*byte);
            while let Some(event) = server.decode(&mut buffer).unwrap() {
                events.push(event);
            }
        }
        assert_eq!(
            events,
            [Event::Ping(b"p".to_vec()), Event::Text("hello".into())]
        );
    }

    #[test]
    fn close_frames_carry_code_and_reason() {
        let (mut server, mut client) = pair(None);
        let mut wire = BytesMut::new();
        server.encode_close(Some(1012), "server-restart", &mut wire);
        assert_eq!(
            decode_all(&mut client, &wire).unwrap(),
            [Event::Close(Some(1012), "server-restart".into())]
        );
        let mut wire = BytesMut::new();
        client.encode_close(None, "", &mut wire);
        assert_eq!(
            decode_all(&mut server, &wire).unwrap(),
            [Event::Close(None, String::new())]
        );
    }

    #[test]
    fn rejects_protocol_violations() {
        let (mut server, mut client) = pair(None);
        let code = |bytes: &[u8], server: &mut Codec| decode_all(server, bytes).unwrap_err().code;
        // Unmasked client frame.
        assert_eq!(code(&[0x81, 0x01, b'a'], &mut server), 1002);
        let mut wire = BytesMut::new();
        client.encode_frame(0x80 | OP_CONTINUATION, b"x", &mut wire);
        assert_eq!(code(&wire, &mut Codec::new(Role::Server, None, 8192)), 1002);
        let mut wire = BytesMut::new();
        client.encode_frame(0x80 | 0x40 | OP_TEXT, b"x", &mut wire);
        assert_eq!(
            code(&wire, &mut Codec::new(Role::Server, None, 8192)),
            1002,
            "RSV1 without deflate"
        );
        let mut wire = BytesMut::new();
        client.encode_frame(0x80 | OP_TEXT, &[0xff, 0xfe], &mut wire);
        assert_eq!(code(&wire, &mut Codec::new(Role::Server, None, 8192)), 1007);
        let mut wire = BytesMut::new();
        client.encode_frame(0x80 | OP_CLOSE, &[0x03, 0xed], &mut wire);
        assert_eq!(
            code(&wire, &mut Codec::new(Role::Server, None, 8192)),
            1002,
            "close code 1005 is reserved"
        );
        // An oversized frame is refused from its header alone.
        assert_eq!(
            code(
                &[0x81, 0xff, 0, 0, 0, 0, 0, 1, 0, 0],
                &mut Codec::new(Role::Server, None, 8192)
            ),
            1009
        );
        let mut wire = BytesMut::new();
        client.encode_frame(OP_TEXT, &[b'a'; 5000], &mut wire);
        client.encode_frame(0x80 | OP_CONTINUATION, &[b'a'; 5000], &mut wire);
        assert_eq!(
            code(&wire, &mut Codec::new(Role::Server, None, 8192)),
            1009,
            "fragments add up"
        );
    }

    #[test]
    fn compressed_messages_are_bounded_after_inflation() {
        let params = DeflateParams::default();
        let (mut server, _) = pair(Some(&params));
        let mut sender = Codec::new(Role::Client, Some(&params), 1 << 20);
        let mut wire = BytesMut::new();
        sender.encode_text(&"a".repeat(9000), &mut wire);
        assert!(wire.len() < 200);
        assert_eq!(decode_all(&mut server, &wire).unwrap_err().code, 1009);
    }
}
