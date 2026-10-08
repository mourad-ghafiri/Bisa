//! The flight recorder: the last events at `debug` and above, kept in
//! memory and written nowhere — until something dies. A crash report carries
//! them, so an `error` line at the default level (errors only) does not
//! stand alone: what the process was doing before is in the report. The
//! first file a process opens also takes what was said before there was a
//! file — the *replay* — so the shell's words about a node that never
//! started are not lost to stderr.
//!
//! A ring of [`RECORDER_CAPACITY`] entries behind one mutex; an event is one
//! visit of its fields into bounded strings. Nothing here formats a payload:
//! the fields are what the site said.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer};

/// How many events the ring keeps.
pub const RECORDER_CAPACITY: usize = 256;

/// The lowest level the ring records — `trace` never reaches it.
pub const RECORDER_FLOOR: LevelFilter = LevelFilter::DEBUG;

/// The most of a message or a field the ring keeps.
const MAX_TEXT: usize = 1024;
/// The most fields of one event the ring keeps.
const MAX_FIELDS: usize = 32;

/// One recorded event: when, how loud, from where, the words and the
/// fields as their words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorded {
    /// RFC 3339, UTC.
    pub at: String,
    /// The level's word, upper case as the file layer writes it.
    pub level: String,
    pub target: String,
    pub message: String,
    /// The event's fields but `message`, in the order the site gave them.
    pub fields: Vec<(String, String)>,
}

#[derive(Default)]
struct Ring {
    entries: VecDeque<Recorded>,
    /// How many of the entries, counted from the back, were said before
    /// any file was open — the replay's share.
    unwritten: usize,
}

/// The ring, shared by the layer that fills it and the handle that reads
/// it. Cheap to clone.
#[derive(Clone, Default)]
pub struct Recorder {
    ring: Arc<Mutex<Ring>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every entry, oldest first.
    pub fn snapshot(&self) -> Vec<Recorded> {
        self.lock().entries.iter().cloned().collect()
    }

    /// The entries said before any file was open, oldest first — taken
    /// once: a second call answers nothing.
    /// The backlog is owed to no file: the log was turned off.
    pub fn forget_unwritten(&self) {
        drop(self.take_unwritten());
    }

    pub fn take_unwritten(&self) -> Vec<Recorded> {
        let mut ring = self.lock();
        let n = ring.unwritten.min(ring.entries.len());
        ring.unwritten = 0;
        let start = ring.entries.len() - n;
        ring.entries.range(start..).cloned().collect()
    }

    fn push(&self, entry: Recorded, written: bool) {
        let mut ring = self.lock();
        if ring.entries.len() == RECORDER_CAPACITY {
            ring.entries.pop_front();
            // The dropped entry was the oldest; it was unwritten only if
            // every entry was.
            if ring.unwritten > ring.entries.len() {
                ring.unwritten = ring.entries.len();
            }
        }
        ring.entries.push_back(entry);
        if !written {
            ring.unwritten += 1;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Ring> {
        self.ring.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Whether what is said now is settled — a file is open, or the log is off
/// and nothing said is owed to a later file. Set by the handle, read by the
/// recorder on every event so the first replay knows what it carries.
#[derive(Clone, Default)]
pub struct WrittenFlag(Arc<std::sync::atomic::AtomicBool>);

impl WrittenFlag {
    pub fn set(&self, on: bool) {
        self.0.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The recorder and the flag together: what [`crate::build`] layers on.
pub struct Recording {
    pub recorder: Recorder,
    pub written: WrittenFlag,
}

impl Recording {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::new(),
            written: WrittenFlag::default(),
        }
    }

    pub fn layer(&self) -> RecordingLayer {
        RecordingLayer {
            recorder: self.recorder.clone(),
            written: self.written.clone(),
        }
    }
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}

/// The layer as it sits on the subscriber: one visit per event into the
/// ring, marked written when a file was open to take it.
pub struct RecordingLayer {
    recorder: Recorder,
    written: WrittenFlag,
}

impl<S: Subscriber> Layer<S> for RecordingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        self.recorder.push(recorded(event), self.written.get());
    }
}

/// The event as the ring keeps it.
pub(crate) fn recorded(event: &Event<'_>) -> Recorded {
    let meta = event.metadata();
    let mut visitor = Collect::default();
    event.record(&mut visitor);
    Recorded {
        at: crate::stamp::rfc3339(SystemTime::now()),
        level: level_word(*meta.level()).to_string(),
        target: meta.target().to_string(),
        message: visitor.message,
        fields: visitor.fields,
    }
}

pub(crate) fn level_word(level: Level) -> &'static str {
    match level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "TRACE",
    }
}

#[derive(Default)]
struct Collect {
    message: String,
    fields: Vec<(String, String)>,
}

impl Collect {
    fn take(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = bounded(value);
        } else if self.fields.len() < MAX_FIELDS {
            self.fields.push((field.name().to_string(), bounded(value)));
        }
    }
}

impl Visit for Collect {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.take(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.take(field, value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.take(field, value.to_string());
    }
}

/// The text, cut at [`MAX_TEXT`] characters with an ellipsis.
pub(crate) fn bounded(text: String) -> String {
    if text.len() <= MAX_TEXT {
        return text;
    }
    let mut cut: String = text.chars().take(MAX_TEXT).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    #[test]
    fn the_ring_keeps_the_last_entries_and_the_unwritten_count_follows() {
        let recording = Recording::new();
        let _guard = tracing_subscriber::registry()
            .with(recording.layer().with_filter(RECORDER_FLOOR))
            .set_default();
        for i in 0..(RECORDER_CAPACITY + 5) {
            tracing::debug!(target: "ring", i, "entry");
        }
        tracing::trace!(target: "ring", "never recorded");
        let all = recording.recorder.snapshot();
        assert_eq!(all.len(), RECORDER_CAPACITY);
        assert_eq!(all[0].fields, vec![("i".to_string(), "5".to_string())]);
        assert_eq!(all[0].level, "DEBUG");
        assert_eq!(all[0].target, "ring");
        assert_eq!(all[0].message, "entry");
        assert!(all[0].at.ends_with('Z'));

        // Nothing was written yet: every kept entry is unwritten, once.
        let unwritten = recording.recorder.take_unwritten();
        assert_eq!(unwritten.len(), RECORDER_CAPACITY);
        assert!(recording.recorder.take_unwritten().is_empty());

        // With a file open, an entry is written as it is said.
        recording.written.set(true);
        tracing::warn!(target: "ring", "after the open");
        assert!(recording.recorder.take_unwritten().is_empty());
        assert_eq!(recording.recorder.snapshot().len(), RECORDER_CAPACITY);
    }

    #[test]
    fn a_message_and_a_field_are_bounded_and_an_error_is_its_sentence() {
        let recording = Recording::new();
        let _guard = tracing_subscriber::registry()
            .with(recording.layer())
            .set_default();
        let long = "x".repeat(MAX_TEXT + 10);
        let err = std::io::Error::other("the disk said no");
        tracing::error!(target: "ring", long = %long, error = &err as &dyn std::error::Error, "{long}");
        let entry = recording.recorder.snapshot().pop().unwrap();
        assert_eq!(entry.message.chars().count(), MAX_TEXT + 1);
        assert!(entry.message.ends_with('…'));
        let field = |name: &str| {
            entry
                .fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(field("long").chars().count(), MAX_TEXT + 1);
        assert_eq!(field("error"), "the disk said no");
    }

    // added by the coverage pass: b6-recorder.rs
    #[test]
    fn a_default_recording_is_a_new_one_and_every_level_has_its_word() {
        let recording = Recording::default();
        assert!(recording.recorder.snapshot().is_empty());
        assert!(!recording.written.get());
        assert_eq!(level_word(Level::TRACE), "TRACE");
        assert_eq!(level_word(Level::INFO), "INFO");
    }
}
