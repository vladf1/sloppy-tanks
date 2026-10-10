//! `Sec-WebSocket-Extensions` negotiation for permessage-deflate (RFC 7692 section 7.1).

/// Accepted permessage-deflate parameters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeflateParams {
    /// The server resets its compressor after every message.
    pub server_no_context_takeover: bool,
    /// The client resets its compressor after every message, so the server may reset
    /// its decompressor too.
    pub client_no_context_takeover: bool,
    /// The server's compression window, when the client limited it.
    pub server_max_window_bits: Option<u8>,
    /// `Some(None)` when the client offered the parameter without a value (it supports
    /// the server limiting its window); `Some(Some(bits))` when it announced its own limit.
    pub client_max_window_bits: Option<Option<u8>>,
}

impl DeflateParams {
    /// The response header value, listing parameters as `ws` does: the ones the client
    /// asked for, except a valueless `client_max_window_bits`.
    pub fn response_header(&self) -> String {
        let mut header = String::from("permessage-deflate");
        if self.server_no_context_takeover {
            header.push_str("; server_no_context_takeover");
        }
        if self.client_no_context_takeover {
            header.push_str("; client_no_context_takeover");
        }
        if let Some(bits) = self.server_max_window_bits {
            header.push_str(&format!("; server_max_window_bits={bits}"));
        }
        if let Some(Some(bits)) = self.client_max_window_bits {
            header.push_str(&format!("; client_max_window_bits={bits}"));
        }
        header
    }
}

/// The header is malformed, or a permessage-deflate offer has an unknown, repeated or
/// invalid parameter; the handshake answers 400 as `ws` does.
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidExtensions;

pub(crate) fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn window_bits(value: &str) -> Result<u8, InvalidExtensions> {
    match value.parse::<u8>() {
        Ok(bits @ 8..=15) if !value.starts_with('0') => Ok(bits),
        _ => Err(InvalidExtensions),
    }
}

/// Parses every `Sec-WebSocket-Extensions` value and accepts the first permessage-deflate
/// offer the server can honour. `Ok(None)` means no compression.
///
/// Like `ws`, only permessage-deflate offers are validated; other extensions are ignored.
/// Unlike `ws`, an offer that limits the server's window to 8 bits is declined rather
/// than accepted: zlib cannot produce raw deflate with a 256-byte window.
pub fn negotiate<'a>(
    headers: impl IntoIterator<Item = &'a str>,
) -> Result<Option<DeflateParams>, InvalidExtensions> {
    let mut accepted = None;
    for header in headers {
        for extension in header.split(',') {
            let extension = extension.trim();
            if extension.is_empty() {
                continue;
            }
            let mut parts = extension.split(';').map(str::trim);
            let name = parts.next().unwrap_or("");
            if !is_token(name) {
                return Err(InvalidExtensions);
            }
            let mut params: Vec<(&str, Option<&str>)> = Vec::new();
            for part in parts {
                let (key, value) = match part.split_once('=') {
                    Some((key, value)) => {
                        let value = value.trim();
                        let value = value
                            .strip_prefix('"')
                            .and_then(|value| value.strip_suffix('"'))
                            .unwrap_or(value);
                        if !is_token(value) {
                            return Err(InvalidExtensions);
                        }
                        (key.trim(), Some(value))
                    }
                    None => (part, None),
                };
                if !is_token(key) {
                    return Err(InvalidExtensions);
                }
                params.push((key, value));
            }
            if name != "permessage-deflate" {
                continue;
            }
            let offer = read_offer(&params)?;
            if accepted.is_none() && offer.server_max_window_bits != Some(8) {
                accepted = Some(offer);
            }
        }
    }
    Ok(accepted)
}

fn read_offer(params: &[(&str, Option<&str>)]) -> Result<DeflateParams, InvalidExtensions> {
    let mut offer = DeflateParams::default();
    let mut seen: Vec<&str> = Vec::new();
    for (key, value) in params {
        if seen.contains(key) {
            return Err(InvalidExtensions);
        }
        seen.push(key);
        match (*key, value) {
            ("server_no_context_takeover", None) => offer.server_no_context_takeover = true,
            ("client_no_context_takeover", None) => offer.client_no_context_takeover = true,
            ("server_max_window_bits", Some(value)) => {
                offer.server_max_window_bits = Some(window_bits(value)?)
            }
            ("client_max_window_bits", None) => offer.client_max_window_bits = Some(None),
            ("client_max_window_bits", Some(value)) => {
                offer.client_max_window_bits = Some(Some(window_bits(value)?))
            }
            _ => return Err(InvalidExtensions),
        }
    }
    Ok(offer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_browser_offer_with_context_takeover() {
        let offer = negotiate(["permessage-deflate; client_max_window_bits"])
            .unwrap()
            .unwrap();
        assert_eq!(offer.client_max_window_bits, Some(None));
        assert!(!offer.server_no_context_takeover);
        assert_eq!(offer.response_header(), "permessage-deflate");
    }

    #[test]
    fn echoes_the_limits_a_client_asks_for() {
        let offer = negotiate([
            "permessage-deflate; server_no_context_takeover; server_max_window_bits=10; client_max_window_bits=\"12\"",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            offer.response_header(),
            "permessage-deflate; server_no_context_takeover; server_max_window_bits=10; client_max_window_bits=12"
        );
    }

    #[test]
    fn takes_the_first_acceptable_offer_across_header_lines() {
        let offer = negotiate([
            "x-webkit-deflate-frame",
            "permessage-deflate; server_max_window_bits=8, permessage-deflate; client_no_context_takeover",
        ])
        .unwrap()
        .unwrap();
        assert!(offer.client_no_context_takeover);
        assert_eq!(offer.server_max_window_bits, None);
    }

    #[test]
    fn no_offer_means_no_compression() {
        assert_eq!(negotiate([]), Ok(None));
        assert_eq!(negotiate(["foo; bar=1"]), Ok(None));
        assert_eq!(
            negotiate(["permessage-deflate; server_max_window_bits=8"]),
            Ok(None)
        );
    }

    #[test]
    fn rejects_invalid_deflate_parameters() {
        for header in [
            "permessage-deflate; server_max_window_bits",
            "permessage-deflate; server_max_window_bits=16",
            "permessage-deflate; client_max_window_bits=7",
            "permessage-deflate; server_no_context_takeover=1",
            "permessage-deflate; unknown",
            "permessage-deflate; client_no_context_takeover; client_no_context_takeover",
            "permessage deflate",
            "permessage-deflate; a=\"b c\"",
        ] {
            assert_eq!(negotiate([header]), Err(InvalidExtensions), "{header}");
        }
    }
}
