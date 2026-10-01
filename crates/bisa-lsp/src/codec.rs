//! JSON-RPC framing as LSP does it: `Content-Length: N\r\n\r\n<json>`.
//! Incremental, because stdout arrives in whatever chunks the pipe gives —
//! and bounded, because the length is the server's word, not the platform's.

use serde_json::Value;

/// The most a header may take before the stream is not LSP at all.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;

/// The most one message body may weigh: 16 MiB — the order of the harness
/// line cap. What the platform sends is a `didOpen` of a file the editor
/// edits (2 MiB, escaped to at most twice that); what it can show back — a
/// completion list, a set of diagnostics — is far smaller. A server naming
/// more is refused before a byte of the body is buffered.
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Why the stream is not one the platform reads. Every variant is fatal for
/// the server it came from: the supervisor turns it into `Failed`, never
/// skips a frame and guesses at the next.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("no header in the first {MAX_HEADER_BYTES} bytes")]
    NoHeader,
    #[error("malformed header line {0:?}")]
    MalformedHeaderLine(String),
    #[error("header without Content-Length")]
    MissingLength,
    #[error("Content-Length is not a length: {0:?}")]
    MalformedLength(String),
    #[error("Content-Length {length} is over the {max}-byte limit")]
    BodyTooLarge { length: u64, max: usize },
    #[error("body is not JSON: {0}")]
    NotJson(String),
}

/// Encode one message.
pub fn encode(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).unwrap_or_else(|_| b"{}".to_vec());
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// Bytes in, messages out. A header that is not a header, a length that is
/// not one or is over the cap, or a body that is not JSON, is an error the
/// caller turns into `Failed` — never skipped.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete message, `Ok(None)` when more bytes are needed.
    pub fn next_message(&mut self) -> Result<Option<Value>, DecodeError> {
        let Some(header_end) = find(&self.buf, b"\r\n\r\n") else {
            // Cap the header search so garbage cannot grow the buffer forever.
            if self.buf.len() > MAX_HEADER_BYTES {
                return Err(DecodeError::NoHeader);
            }
            return Ok(None);
        };
        let header = String::from_utf8_lossy(&self.buf[..header_end]).into_owned();
        let mut length: Option<u64> = None;
        for line in header.split("\r\n") {
            let Some((name, value)) = line.split_once(':') else {
                if line.trim().is_empty() {
                    continue;
                }
                return Err(DecodeError::MalformedHeaderLine(line.to_string()));
            };
            if name.trim().eq_ignore_ascii_case("content-length") {
                let raw = value.trim();
                length = Some(
                    raw.parse::<u64>()
                        .map_err(|_| DecodeError::MalformedLength(raw.to_string()))?,
                );
            }
        }
        let Some(length) = length else {
            return Err(DecodeError::MissingLength);
        };
        if length > MAX_BODY_BYTES as u64 {
            return Err(DecodeError::BodyTooLarge {
                length,
                max: MAX_BODY_BYTES,
            });
        }
        let start = header_end + 4;
        // Under the cap the sum cannot overflow; the check is the invariant,
        // not a bet on the constant.
        let end = start
            .checked_add(length as usize)
            .ok_or(DecodeError::BodyTooLarge {
                length,
                max: MAX_BODY_BYTES,
            })?;
        if self.buf.len() < end {
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(&self.buf[start..end])
            .map_err(|e| DecodeError::NotJson(e.to_string()))?;
        self.buf.drain(..end);
        Ok(Some(value))
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_round_trips_and_arrives_in_pieces() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"é": "ü"}});
        let bytes = encode(&msg);
        assert!(bytes.starts_with(b"Content-Length: "));
        let mut d = Decoder::default();
        // Feed one byte at a time; the message appears only when complete.
        let mut got = None;
        for b in &bytes {
            d.push(&[*b]);
            if let Some(v) = d.next_message().unwrap() {
                got = Some(v);
            }
        }
        assert_eq!(got.unwrap(), msg);
        assert_eq!(d.next_message().unwrap(), None);
    }

    #[test]
    fn two_messages_in_one_chunk_come_out_in_order() {
        let a = json!({"id": 1});
        let b = json!({"id": 2});
        let mut bytes = encode(&a);
        bytes.extend(encode(&b));
        let mut d = Decoder::default();
        d.push(&bytes);
        assert_eq!(d.next_message().unwrap(), Some(a));
        assert_eq!(d.next_message().unwrap(), Some(b));
        assert_eq!(d.next_message().unwrap(), None);
    }

    #[test]
    fn garbage_is_an_error_not_a_guess() {
        let mut d = Decoder::default();
        d.push(b"Content-Length: 5\r\n\r\nnotjs");
        assert!(matches!(d.next_message(), Err(DecodeError::NotJson(_))));
        let mut d = Decoder::default();
        d.push(b"Nonsense\r\n\r\n{}");
        assert!(matches!(
            d.next_message(),
            Err(DecodeError::MalformedHeaderLine(_))
        ));
        let mut d = Decoder::default();
        d.push(b"Content-Type: x\r\n\r\n{}");
        assert_eq!(d.next_message(), Err(DecodeError::MissingLength));
    }

    #[test]
    fn a_content_length_that_is_not_a_number_is_a_malformed_length_not_a_missing_header() {
        for raw in ["many", "-5", "123456789012345678901234567890"] {
            let mut d = Decoder::default();
            d.push(format!("Content-Length: {raw}\r\n\r\n{{}}").as_bytes());
            assert_eq!(
                d.next_message(),
                Err(DecodeError::MalformedLength(raw.to_string())),
                "{raw}"
            );
        }
    }

    #[test]
    fn a_content_length_over_the_cap_is_refused_before_any_body_is_buffered() {
        for length in [MAX_BODY_BYTES as u64 + 1, u64::MAX] {
            let mut d = Decoder::default();
            d.push(format!("Content-Length: {length}\r\n\r\n").as_bytes());
            assert_eq!(
                d.next_message(),
                Err(DecodeError::BodyTooLarge {
                    length,
                    max: MAX_BODY_BYTES
                }),
                "{length}"
            );
        }
    }

    #[test]
    fn a_content_length_at_the_cap_waits_for_its_body() {
        let mut d = Decoder::default();
        d.push(format!("Content-Length: {MAX_BODY_BYTES}\r\n\r\n").as_bytes());
        assert_eq!(d.next_message(), Ok(None), "the cap is inclusive");
    }
}
