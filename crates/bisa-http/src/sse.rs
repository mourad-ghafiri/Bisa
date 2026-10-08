//! Server-sent events, read off whatever chunks a transport delivers.
//!
//! Three readers used to parse this wire on their own — a harness's own event
//! server pulled by the engine, the node's `/events` tailed by the CLI, an MCP
//! server's HTTP+SSE stream probed — and each decoded every chunk with
//! `from_utf8_lossy` before looking for a blank line, so a multi-byte character
//! split across two reads became U+FFFD, and none of them bounded the bytes it
//! kept. One reader now: bytes in, events out, decoded once per complete
//! frame, capped.
//!
//! The grammar is the WHATWG one (`https://html.spec.whatwg.org/multipage/server-sent-events.html`,
//! § Parsing an event stream): a line ends at `\r\n`, `\n` or `\r`; an empty
//! line dispatches the event; `data:` lines are joined with `\n`, one leading
//! space stripped from each; `event:` names it (`message` when none); `id:`,
//! `retry:` and comment lines (`:`) are read past; a frame with no `data:`
//! line is nothing — a keep-alive.

use std::collections::VecDeque;

/// The most one event may weigh before its stream is a fault: 1 MiB. An event
/// here is one JSON value — a session report, a node event, an MCP answer —
/// never a file.
pub const MAX_SSE_FRAME_BYTES: usize = 1024 * 1024;

/// One server-sent event: its name — `message` when the stream gave none —
/// and its `data:` lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// A frame ran past [`MAX_SSE_FRAME_BYTES`] without ending: the stream is not
/// one the platform reads, and the caller ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a server-sent event ran past {max} bytes without ending")]
pub struct SseOverflow {
    pub max: usize,
}

/// Bytes in, events out — over whatever chunks the transport delivers. A
/// frame is decoded only once its blank line has arrived, so a character
/// split across two chunks is one character; the events a push completes
/// wait in order for [`SseFrames::next_event`].
#[derive(Debug, Default)]
pub struct SseFrames {
    buf: Vec<u8>,
    ready: VecDeque<SseEvent>,
}

impl SseFrames {
    /// Feed bytes; every frame they complete is parsed into the queue. Past
    /// the cap with no frame end in sight the buffer is dropped and this is
    /// the error the caller ends the stream on — the next push starts clean.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), SseOverflow> {
        self.buf.extend_from_slice(bytes);
        while let Some((end, separator)) = frame_end(&self.buf) {
            let frame: Vec<u8> = self.buf.drain(..end + separator).collect();
            if let Some(event) = parse_frame(&frame[..end]) {
                self.ready.push_back(event);
            }
        }
        if self.buf.len() > MAX_SSE_FRAME_BYTES {
            self.buf.clear();
            return Err(SseOverflow {
                max: MAX_SSE_FRAME_BYTES,
            });
        }
        Ok(())
    }

    /// The next complete event, in the order the stream sent them.
    pub fn next_event(&mut self) -> Option<SseEvent> {
        self.ready.pop_front()
    }
}

/// What a byte begins: a line end, the first half of one that may still be
/// `\r\n` once the next byte arrives, or an ordinary byte.
enum Mark {
    Byte,
    Incomplete,
    End(usize),
}

fn mark_at(buf: &[u8], i: usize) -> Mark {
    match buf[i] {
        b'\n' => Mark::End(1),
        b'\r' => match buf.get(i + 1) {
            None => Mark::Incomplete,
            Some(b'\n') => Mark::End(2),
            Some(_) => Mark::End(1),
        },
        _ => Mark::Byte,
    }
}

/// Where the first frame ends — the index of the blank line's first
/// terminator — and how many bytes the two terminators take. `None` while the
/// frame is still arriving, including when the buffer ends in a lone `\r`
/// that may become `\r\n`.
fn frame_end(buf: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < buf.len() {
        match mark_at(buf, i) {
            Mark::Byte => i += 1,
            Mark::Incomplete => return None,
            Mark::End(first) => {
                let next = i + first;
                if next >= buf.len() {
                    return None;
                }
                match mark_at(buf, next) {
                    Mark::End(second) => return Some((i, first + second)),
                    Mark::Incomplete => return None,
                    Mark::Byte => i = next,
                }
            }
        }
    }
    None
}

/// One complete frame's lines, decoded once, into an event — or nothing when
/// no `data:` line was in it.
fn parse_frame(frame: &[u8]) -> Option<SseEvent> {
    let text = String::from_utf8_lossy(frame)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut event = String::from("message");
    let mut data: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "data" => data.push(value),
            // An empty name resets to the default, as the spec says.
            "event" => {
                event = if value.is_empty() {
                    "message".into()
                } else {
                    value.into()
                }
            }
            _ => {}
        }
    }
    if data.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(frames: &mut SseFrames) -> Vec<SseEvent> {
        let mut out = Vec::new();
        while let Some(e) = frames.next_event() {
            out.push(e);
        }
        out
    }

    fn message(data: &str) -> SseEvent {
        SseEvent {
            event: "message".into(),
            data: data.into(),
        }
    }

    #[test]
    fn a_multibyte_character_split_across_two_pushes_is_one_character() {
        let mut f = SseFrames::default();
        f.push(b"data: caf\xC3").unwrap();
        assert_eq!(events(&mut f), vec![], "nothing before the blank line");
        f.push(b"\xA9\n\n").unwrap();
        assert_eq!(events(&mut f), vec![message("café")]);
    }

    #[test]
    fn crlf_and_cr_separators_end_a_frame_like_a_blank_line() {
        let mut f = SseFrames::default();
        f.push(b"data: a\r\n\r\ndata: b\r\rdata: c\n\n").unwrap();
        assert_eq!(
            events(&mut f),
            vec![message("a"), message("b"), message("c")]
        );
    }

    #[test]
    fn a_lone_carriage_return_at_the_end_waits_for_the_byte_that_may_follow_it() {
        let mut f = SseFrames::default();
        f.push(b"data: a\r").unwrap();
        assert_eq!(events(&mut f), vec![]);
        f.push(b"\n\r").unwrap();
        assert_eq!(
            events(&mut f),
            vec![],
            "the second terminator may still be CRLF"
        );
        f.push(b"\n").unwrap();
        assert_eq!(events(&mut f), vec![message("a")]);
    }

    #[test]
    fn several_frames_in_one_push_come_out_in_order() {
        let mut f = SseFrames::default();
        f.push(b"data: 1\n\ndata: 2\n\ndata: 3\n\n").unwrap();
        assert_eq!(
            events(&mut f),
            vec![message("1"), message("2"), message("3")]
        );
    }

    #[test]
    fn a_data_line_with_or_without_a_space_after_the_colon_reads_the_same() {
        let mut f = SseFrames::default();
        f.push(b"data:{\"a\":1}\n\ndata: {\"a\":1}\n\ndata:  two\n\n")
            .unwrap();
        assert_eq!(
            events(&mut f),
            vec![message("{\"a\":1}"), message("{\"a\":1}"), message(" two")],
            "one space is the separator; a second is data"
        );
    }

    #[test]
    fn two_data_lines_are_joined_with_a_newline_and_the_event_name_is_read() {
        let mut f = SseFrames::default();
        f.push(b"event: endpoint\ndata: /m\ndata: ?s=1\n\nevent:\ndata: x\n\n")
            .unwrap();
        assert_eq!(
            events(&mut f),
            vec![
                SseEvent {
                    event: "endpoint".into(),
                    data: "/m\n?s=1".into()
                },
                message("x"),
            ],
            "an empty event name is the default name"
        );
    }

    #[test]
    fn comments_id_and_retry_lines_are_read_past_and_a_frame_without_data_is_nothing() {
        let mut f = SseFrames::default();
        f.push(b": ping\n\nid: 7\nretry: 100\n\n: keep\ndata: alive\n\n")
            .unwrap();
        assert_eq!(events(&mut f), vec![message("alive")]);
    }

    #[test]
    fn a_frame_over_the_cap_is_an_overflow_and_the_buffer_is_cleared() {
        let mut f = SseFrames::default();
        let long = vec![b'a'; MAX_SSE_FRAME_BYTES + 1];
        assert_eq!(
            f.push(&long),
            Err(SseOverflow {
                max: MAX_SSE_FRAME_BYTES
            })
        );
        f.push(b"data: ok\n\n").unwrap();
        assert_eq!(events(&mut f), vec![message("ok")], "the next frame parses");
    }

    #[test]
    fn a_stream_split_at_every_byte_reassembles() {
        let stream = "event: e\ndata: {\"k\":\"ü\"}\r\ndata: more\r\n\r\ndata: last\n\n".as_bytes();
        let mut f = SseFrames::default();
        for b in stream {
            f.push(&[*b]).unwrap();
        }
        assert_eq!(
            events(&mut f),
            vec![
                SseEvent {
                    event: "e".into(),
                    data: "{\"k\":\"ü\"}\nmore".into()
                },
                message("last"),
            ]
        );
    }

    // added by the coverage pass: b6-sse.rs
    #[test]
    fn a_field_line_without_a_colon_is_the_field_with_nothing_after_it() {
        let mut f = SseFrames::default();
        f.push(b"data\n\nevent\ndata: x\n\n").unwrap();
        assert_eq!(
            events(&mut f),
            vec![message(""), message("x")],
            "a bare `data` is an empty data line; a bare `event` is the default name"
        );
    }
}
