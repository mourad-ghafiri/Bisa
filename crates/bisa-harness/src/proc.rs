//! Shared subprocess plumbing for line-framed (NDJSON/JSONL) harness CLIs.
//!
//! Spawns the harness with piped stdio — **in a process group of its own**,
//! so a stop reaches what the harness started: the commands its tools run,
//! the injected MCP server, a dev server — runs a reader task that frames
//! stdout into lines (parsed as JSON when they are JSON), keeps a bounded
//! tail of stderr for diagnostics, tracks last-activity for idle timeouts,
//! and ends the whole group when told to, or when the handle is dropped
//! with the child still running.
//!
//! Ending is two-phased everywhere ([`ProcHandle::terminate`],
//! [`ProcGroup::terminate`]): `SIGTERM` to the group, a grace to leave, then
//! `SIGKILL`. [`ABORT_GRACE`] bounds a stop and [`DISPOSE_GRACE`] a let-go,
//! both under the engine's own wait for a stopped session's process to be
//! gone.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use crate::error::HarnessError;
use bisa_core::sync::Locked;

const STDERR_TAIL_LINES: usize = 64;
const LINE_CHANNEL_CAPACITY: usize = 1024;

/// The longest line of a harness's output that is kept whole. A harness
/// frames its events one JSON value a line, and a tool's output rides inside
/// one: megabytes are ordinary, and past this a line is cut rather than grown
/// without end in memory.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// How long a stopped harness has to leave on its own: told the way it
/// understands — a `session/cancel`, an `abort` command — and `SIGTERM` to
/// its whole process group; past this, `SIGKILL`. Under the engine's wait
/// for a stop (five seconds), so a verb that stops a session returns to a
/// process that is gone.
pub const ABORT_GRACE: Duration = Duration::from_secs(3);

/// How long a disposed harness has to leave on EOF before its group is
/// killed: a CLI that ignores a closed stdin is not left to run on.
pub const DISPOSE_GRACE: Duration = Duration::from_secs(5);

/// How long after `SIGKILL` a group is given for its leader to be reaped by
/// whoever holds the child — the driver, on stdout closing.
const REAP_GRACE: Duration = Duration::from_secs(1);

/// How often a wait for a group to be gone looks.
const GONE_POLL: Duration = Duration::from_millis(50);

/// One line of a child's output, as text.
#[derive(Debug, PartialEq, Eq)]
pub struct ReadLine {
    pub text: String,
    /// The line was longer than the limit: `text` is its head, the rest was
    /// read and let go.
    pub cut: bool,
}

/// The next line of `reader`, or `None` at its end. Unlike `lines()`, this
/// never stops at a byte that is no UTF-8 — it is replaced, and the stream
/// goes on, since a reader that gave up there left the child blocked on a
/// full pipe with nobody saying why — and never holds more than `max` bytes
/// of one line. A last line with no newline is a line.
pub async fn next_line<R>(reader: &mut R, max: usize) -> std::io::Result<Option<ReadLine>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut kept: Vec<u8> = Vec::new();
    let mut cut = false;
    let mut read_any = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            break;
        }
        read_any = true;
        let (take, ended) = match available.iter().position(|b| *b == b'\n') {
            Some(at) => (at, true),
            None => (available.len(), false),
        };
        let room = max.saturating_sub(kept.len());
        kept.extend_from_slice(&available[..take.min(room)]);
        cut |= take > room;
        reader.consume(take + usize::from(ended));
        if ended {
            break;
        }
    }
    if !read_any {
        return Ok(None);
    }
    if kept.last() == Some(&b'\r') {
        kept.pop();
    }
    Ok(Some(ReadLine {
        text: String::from_utf8_lossy(&kept).into_owned(),
        cut,
    }))
}

/// What to spawn.
#[derive(Debug, Clone)]
pub struct ProcSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// Names the child must not inherit — a proxy the node was given that
    /// the person said not to use. Applied after `env`, and after the
    /// control-plane scrub, which needs no asking.
    pub env_remove: Vec<String>,
    pub cwd: Option<PathBuf>,
}

impl ProcSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            env_remove: Vec::new(),
            cwd: None,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: Into<String>>(mut self, args: I) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.env.insert(k.into(), v.into());
        self
    }

    pub fn env_remove(mut self, k: impl Into<String>) -> Self {
        self.env_remove.push(k.into());
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }
}

/// One framed stdout line.
#[derive(Debug, Clone)]
pub enum Line {
    Json(serde_json::Value),
    Text(String),
}

/// The process group a harness was spawned into — its own, the child its
/// leader — as a handle anything may signal without holding the child: a
/// session facade whose child a driver task owns, a timer, a drop. A stop
/// through it reaches the harness's tools, the shells they run and the MCP
/// server it was handed, which die with the group and never with the one
/// pid. A process that left the group (`setsid`) is beyond it — the one
/// limit of a group, and none of ours does.
#[derive(Debug)]
pub struct ProcGroup {
    #[cfg(unix)]
    pgid: Option<rustix::process::Pid>,
    /// The leader was reaped by whoever owned the child after an explicit
    /// end: the group's number may be somebody else's from here on, so
    /// nothing is signalled again.
    reaped: AtomicBool,
}

impl ProcGroup {
    /// No group: every signal is nothing — a session with no local process.
    pub fn none() -> Self {
        Self {
            #[cfg(unix)]
            pgid: None,
            reaped: AtomicBool::new(true),
        }
    }

    fn of_child(child: &Child) -> Self {
        #[cfg(unix)]
        let pgid = child
            .id()
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(rustix::process::Pid::from_raw);
        Self {
            #[cfg(unix)]
            pgid,
            reaped: AtomicBool::new(false),
        }
    }

    #[cfg(unix)]
    fn signal(&self, signal: rustix::process::Signal) -> bool {
        if self.reaped.load(Ordering::SeqCst) {
            return false;
        }
        match self.pgid {
            Some(pgid) => rustix::process::kill_process_group(pgid, signal).is_ok(),
            None => false,
        }
    }

    /// `SIGTERM` to the group. Whether anything was there to hear it.
    pub fn term(&self) -> bool {
        #[cfg(unix)]
        {
            self.signal(rustix::process::Signal::TERM)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// `SIGKILL` to the group. Whether anything was there.
    pub fn kill(&self) -> bool {
        #[cfg(unix)]
        {
            self.signal(rustix::process::Signal::KILL)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Whether anything of the group is still there — a member, or the
    /// leader not yet reaped.
    pub fn alive(&self) -> bool {
        #[cfg(unix)]
        {
            if self.reaped.load(Ordering::SeqCst) {
                return false;
            }
            self.pgid
                .is_some_and(|pgid| rustix::process::test_kill_process_group(pgid).is_ok())
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// The leader was reaped after an explicit end: the group is nobody's
    /// to signal from here on.
    pub fn reaped(&self) {
        self.reaped.store(true, Ordering::SeqCst);
    }

    /// Wait until nothing of the group is left, or `within` passes. Whether
    /// it is gone.
    pub async fn wait_gone(&self, within: Duration) -> bool {
        let until = tokio::time::Instant::now() + within;
        while self.alive() {
            if tokio::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(GONE_POLL).await;
        }
        true
    }

    /// End the group in two steps: `SIGTERM`, `grace` to leave, then
    /// `SIGKILL` — and a moment for the leader to be reaped by whoever
    /// holds the child. Answers whether anything was there to end. What a
    /// session facade does for a child its driver task owns.
    pub async fn terminate(&self, grace: Duration) -> bool {
        if !self.term() {
            return false;
        }
        if !self.wait_gone(grace).await {
            self.kill();
            self.wait_gone(REAP_GRACE).await;
        }
        true
    }

    /// Let the group leave on its own — EOF was given — for `grace`, then
    /// kill what stayed. Answers whether it left on its own.
    pub async fn gone_or_killed(&self, grace: Duration) -> bool {
        if self.wait_gone(grace).await {
            return true;
        }
        self.kill();
        self.wait_gone(REAP_GRACE).await;
        false
    }

    /// End the group the way an abort does, once the cancel the harness
    /// understands was sent and its stdin closed: half of `grace` to leave on
    /// the EOF it was given, then `SIGTERM` and the other half, then
    /// `SIGKILL` and a moment to be reaped. A harness that leaves on EOF —
    /// most CLIs, and an agent that records its own end — ends on its own
    /// terms, as a disposed one does, where `SIGTERM` first cut it off
    /// mid-word; one that ignores both is ended within the same grace.
    /// Answers whether it left on its own.
    pub async fn leave_or_terminate(&self, grace: Duration) -> bool {
        let half = grace / 2;
        if self.wait_gone(half).await {
            return true;
        }
        self.term();
        if !self.wait_gone(half).await {
            self.kill();
            self.wait_gone(REAP_GRACE).await;
        }
        false
    }
}

/// Run a command to completion in a process group of its own, so that
/// however this future ends — the command's exit, a timeout, an abort —
/// everything it started goes with it: the `sh`, and what the `sh` ran.
pub async fn group_output(mut cmd: Command) -> std::io::Result<std::process::Output> {
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.kill_on_drop(true);
    let child = cmd.spawn()?;
    let _sweep = GroupSweep(ProcGroup::of_child(&child));
    child.wait_with_output().await
}

/// The group of a command goes with the future that ran it.
struct GroupSweep(ProcGroup);

impl Drop for GroupSweep {
    fn drop(&mut self) {
        self.0.kill();
    }
}

/// A running harness subprocess.
pub struct ProcHandle {
    child: Child,
    /// The group the child leads — what a stop signals.
    group: Arc<ProcGroup>,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<Line>,
    last_activity: Arc<Mutex<Instant>>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

/// Variables of the node's own environment that no child may inherit: the
/// control-plane token is the node's authority, and a harness — or anything
/// the harness runs — holds only its session's secret. Removed from every
/// child spawned here; the desktop's terminal removes the same names.
pub const SCRUBBED_ENV: &[&str] = &["BISA_API_TOKEN"];

impl ProcHandle {
    pub fn spawn(spec: ProcSpec) -> Result<Self, HarnessError> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Its own group, led by the child: a stop reaches what it starts.
        #[cfg(unix)]
        cmd.process_group(0);
        for name in SCRUBBED_ENV {
            cmd.env_remove(name);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        for name in &spec.env_remove {
            cmd.env_remove(name);
        }
        if let Some(dir) = &spec.cwd {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                HarnessError::unavailable(format!("{} not found on PATH", spec.program))
            } else {
                HarnessError::Io(e)
            }
        })?;

        let group = Arc::new(ProcGroup::of_child(&child));
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
        let (tx, lines) = mpsc::channel(LINE_CHANNEL_CAPACITY);

        // stdout reader: frame into lines, parse JSON opportunistically.
        {
            let last_activity = Arc::clone(&last_activity);
            let program = spec.program.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                loop {
                    let line = match next_line(&mut reader, MAX_LINE_BYTES).await {
                        Ok(Some(read)) => {
                            if read.cut {
                                tracing::warn!(target: "bisa_harness::proc", %program, limit = MAX_LINE_BYTES, "a line of the harness's output was longer than the limit and was cut");
                            }
                            read.text
                        }
                        Ok(None) => break,
                        Err(e) => {
                            // Said, because what follows is a session that hears nothing more.
                            tracing::warn!(target: "bisa_harness::proc", %program, "the harness's output could not be read any further: {e}");
                            break;
                        }
                    };
                    *last_activity.locked() = Instant::now();
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let framed = if trimmed.starts_with('{') || trimmed.starts_with('[') {
                        match serde_json::from_str(trimmed) {
                            Ok(v) => Line::Json(v),
                            Err(_) => Line::Text(line),
                        }
                    } else {
                        Line::Text(line)
                    };
                    if tx.send(framed).await.is_err() {
                        break; // receiver dropped; stop reading
                    }
                }
            });
        }

        // stderr reader: bounded tail + tracing.
        {
            let stderr_tail = Arc::clone(&stderr_tail);
            let program = spec.program.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                // Read to the end whatever it holds: a harness whose stderr
                // nobody drains blocks on its next write.
                while let Ok(Some(read)) = next_line(&mut reader, MAX_LINE_BYTES).await {
                    let line = read.text;
                    tracing::debug!(target: "bisa_harness::proc", %program, "stderr: {line}");
                    let mut tail = stderr_tail.locked();
                    if tail.len() == STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            });
        }

        Ok(Self {
            child,
            group,
            stdin,
            lines,
            last_activity,
            stderr_tail,
        })
    }

    /// The child's process group, to signal without holding the child.
    pub fn group(&self) -> Arc<ProcGroup> {
        Arc::clone(&self.group)
    }

    /// Write one JSON value as a single stdin line.
    pub async fn send_json(&mut self, value: &serde_json::Value) -> Result<(), HarnessError> {
        let mut line =
            serde_json::to_string(value).map_err(|e| HarnessError::protocol(e.to_string()))?;
        line.push('\n');
        self.send_raw(&line).await
    }

    /// Write a raw line (newline appended if missing).
    pub async fn send_line(&mut self, text: &str) -> Result<(), HarnessError> {
        let mut line = text.to_string();
        if !line.ends_with('\n') {
            line.push('\n');
        }
        self.send_raw(&line).await
    }

    async fn send_raw(&mut self, data: &str) -> Result<(), HarnessError> {
        let stdin = self.stdin.as_mut().ok_or(HarnessError::Terminated)?;
        stdin.write_all(data.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Close stdin (EOF to the harness — many CLIs end their run on this).
    pub async fn close_stdin(&mut self) -> Result<(), HarnessError> {
        if let Some(mut stdin) = self.stdin.take() {
            stdin.shutdown().await?;
        }
        Ok(())
    }

    /// Receive the next framed line. `None` = stdout closed (process ending).
    pub async fn recv(&mut self) -> Option<Line> {
        self.lines.recv().await
    }

    /// Receive with a timeout. `Ok(None)` = closed, `Err(_)` = timed out.
    pub async fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Line>, tokio::time::error::Elapsed> {
        tokio::time::timeout(timeout, self.lines.recv()).await
    }

    /// Time since the last stdout activity (for idle-timeout policies).
    pub fn idle_for(&self) -> Duration {
        self.last_activity.locked().elapsed()
    }

    /// Last stderr lines for diagnostics.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail.locked().iter().cloned().collect()
    }

    /// The child's OS pid, while it is running — the root the desktop traces a
    /// listening port back to. `None` once it has exited.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// `SIGKILL` the child and its whole group, and reap the child.
    pub async fn kill(&mut self) -> Result<(), HarnessError> {
        self.group.kill();
        let killed = self.child.kill().await.map_err(HarnessError::Io);
        // A member that lingered — a shell a tool ran, the MCP server — goes
        // with its leader.
        self.group.kill();
        self.group.reaped();
        killed
    }

    /// End the child and its group in two steps: stdin closed and `SIGTERM`
    /// to the group, `grace` to leave, then `SIGKILL`; the child reaped, and
    /// whatever of the group lingered killed with it.
    pub async fn terminate(
        &mut self,
        grace: Duration,
    ) -> Result<std::process::ExitStatus, HarnessError> {
        drop(self.stdin.take());
        self.group.term();
        let status = match tokio::time::timeout(grace, self.child.wait()).await {
            Ok(status) => status,
            Err(_elapsed) => {
                self.group.kill();
                self.child.wait().await
            }
        };
        self.group.kill();
        self.group.reaped();
        status.map_err(HarnessError::Io)
    }

    /// Wait for exit.
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus, HarnessError> {
        self.child.wait().await.map_err(HarnessError::Io)
    }
}

impl Drop for ProcHandle {
    fn drop(&mut self) {
        // A handle let go of with its child still running — a driver task
        // dropped at teardown: tokio kills the leader it holds, and the rest
        // of the group goes here rather than outliving the node. A child that
        // exited and was reaped leaves what it started alone.
        if self.child.id().is_some() {
            self.group.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line of `bytes`, read the way the pump reads a child's output.
    async fn lines_of(bytes: &[u8], max: usize) -> Vec<ReadLine> {
        let mut reader = BufReader::with_capacity(7, bytes);
        let mut out = Vec::new();
        while let Some(line) = next_line(&mut reader, max).await.unwrap() {
            out.push(line);
        }
        out
    }

    #[tokio::test]
    async fn a_byte_that_is_no_text_never_ends_the_reading() {
        // One stray byte in the middle of a stream: what `lines()` stops at,
        // for good, with the child left to block on a full pipe.
        let lines = lines_of(b"{\"a\":1}\nbad \xFF\xFE byte\n{\"b\":2}\n", 1024).await;
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts.len(),
            3,
            "every line is read, the one with the byte too"
        );
        assert_eq!(texts[0], "{\"a\":1}");
        assert!(
            texts[1].starts_with("bad ") && texts[1].ends_with(" byte"),
            "{:?}",
            texts[1]
        );
        assert!(
            texts[1].contains('\u{FFFD}'),
            "the byte is replaced, never dropped silently"
        );
        assert_eq!(texts[2], "{\"b\":2}", "and what follows it is still heard");
        assert!(lines.iter().all(|l| !l.cut));
    }

    #[tokio::test]
    async fn a_line_is_held_to_its_limit_and_the_stream_goes_on_after_it() {
        let mut bytes = vec![b'x'; 5_000];
        bytes.extend_from_slice(b"\nshort\n");
        let lines = lines_of(&bytes, 100).await;
        assert_eq!(lines.len(), 2);
        assert_eq!(
            (lines[0].text.len(), lines[0].cut),
            (100, true),
            "the head is kept, the rest read and let go"
        );
        assert_eq!(
            lines[1],
            ReadLine {
                text: "short".into(),
                cut: false
            }
        );
        // Exactly the limit is whole; one byte more is cut.
        let at = lines_of(&[vec![b'y'; 100], b"\n".to_vec()].concat(), 100).await;
        assert_eq!((at[0].text.len(), at[0].cut), (100, false));
        let over = lines_of(&[vec![b'y'; 101], b"\n".to_vec()].concat(), 100).await;
        assert_eq!((over[0].text.len(), over[0].cut), (100, true));
    }

    #[tokio::test]
    async fn the_last_line_needs_no_newline_and_line_ends_of_either_kind_are_ends() {
        let lines = lines_of(b"one\r\ntwo\n\nlast with no end", 1024).await;
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["one", "two", "", "last with no end"]);
        assert!(lines_of(b"", 1024).await.is_empty(), "nothing is no line");
        assert_eq!(
            lines_of(b"\n", 1024).await,
            vec![ReadLine {
                text: String::new(),
                cut: false
            }]
        );
        // A multi-byte letter split across two reads of the pipe is one letter.
        let lines = lines_of("caf\u{e9} na\u{ef}ve \u{1F600}\n".as_bytes(), 1024).await;
        assert_eq!(lines[0].text, "caf\u{e9} na\u{ef}ve \u{1F600}");
    }

    /// Whatever a child writes — any bytes, cut by the pipe at any size, any
    /// limit — the reader neither panics nor loses a line: there are as many
    /// lines as line ends (one more for a tail with none), no line holds more
    /// than the limit, a line is `cut` exactly when it had more, and with the
    /// limit wide enough the lines joined back are the bytes read as text,
    /// stray bytes replaced, a `\r` before each `\n` let go.
    mod any_bytes {
        use super::*;
        use proptest::prelude::*;

        fn read_all(bytes: &[u8], capacity: usize, max: usize) -> Vec<ReadLine> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a runtime");
            rt.block_on(async {
                let mut reader = BufReader::with_capacity(capacity, bytes);
                let mut out = Vec::new();
                while let Some(line) = next_line(&mut reader, max).await.unwrap() {
                    out.push(line);
                }
                out
            })
        }

        /// The raw lines: the pieces between `\n`s, a tail with no end a
        /// line, nothing after a final `\n`.
        fn raw_lines(bytes: &[u8]) -> Vec<&[u8]> {
            let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
            if bytes.last() == Some(&b'\n') || bytes.is_empty() {
                lines.pop();
            }
            lines
        }

        /// A raw line as text: a trailing `\r` let go, decoded lossily.
        fn as_text(line: &[u8]) -> String {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            String::from_utf8_lossy(line).into_owned()
        }

        proptest! {
            #[test]
            fn every_line_is_read_whole_or_cut_and_none_is_lost(
                bytes in proptest::collection::vec(any::<u8>(), 0..400),
                capacity in 1usize..64,
                max in 1usize..64,
            ) {
                let lines = read_all(&bytes, capacity, max);
                let raw = raw_lines(&bytes);
                prop_assert_eq!(lines.len(), raw.len(), "as many lines as line ends");
                for (line, whole) in lines.iter().zip(&raw) {
                    // Each kept byte decodes to at most one character.
                    prop_assert!(line.text.chars().count() <= max, "never more than the limit: {:?}", line.text);
                    prop_assert_eq!(line.cut, whole.len() > max, "cut exactly when the line had more than the limit");
                    if !line.cut {
                        prop_assert_eq!(&line.text, &as_text(whole));
                    }
                }
            }

            #[test]
            fn with_room_enough_the_lines_are_the_bytes_as_text(
                bytes in proptest::collection::vec(any::<u8>(), 0..400),
                capacity in 1usize..64,
            ) {
                let lines = read_all(&bytes, capacity, 4096);
                let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
                let expected: Vec<String> = raw_lines(&bytes).into_iter().map(as_text).collect();
                prop_assert_eq!(texts, expected);
                prop_assert!(lines.iter().all(|l| !l.cut));
            }
        }
    }

    /// The child asks its own environment for the token by name — `printenv`
    /// with one argument prints that variable or nothing — and gets nothing:
    /// the node's authority stops at the node.
    #[tokio::test]
    async fn a_child_never_inherits_the_control_plane_token() {
        std::env::set_var("BISA_API_TOKEN", "fake-control-plane-token");
        let mut handle = ProcHandle::spawn(ProcSpec::new("printenv").arg("BISA_API_TOKEN"))
            .expect("printenv spawns");
        let mut printed = String::new();
        while let Some(line) = handle.lines.recv().await {
            if let Line::Text(text) = line {
                printed.push_str(&text);
            }
        }
        assert!(
            !printed.contains("fake-control-plane-token"),
            "the child saw the token: {printed:?}"
        );
        assert!(printed.trim().is_empty(), "{printed:?}");
        let status = handle.child.wait().await.expect("the child ends");
        assert!(
            !status.success(),
            "printenv exits non-zero for an unset name"
        );
        assert_eq!(SCRUBBED_ENV, &["BISA_API_TOKEN"]);
    }

    /// A name the spec removes is gone from the child — the way a proxy the
    /// node inherited stays out of a session when the person said `none`.
    #[tokio::test]
    async fn a_removed_name_is_gone_from_the_child() {
        std::env::set_var("BISA_TEST_PROXY_NAME", "http://proxy.example:3128");
        let mut handle = ProcHandle::spawn(
            ProcSpec::new("printenv")
                .arg("BISA_TEST_PROXY_NAME")
                .env_remove("BISA_TEST_PROXY_NAME"),
        )
        .expect("printenv spawns");
        let mut printed = String::new();
        while let Some(line) = handle.lines.recv().await {
            if let Line::Text(text) = line {
                printed.push_str(&text);
            }
        }
        assert!(
            printed.trim().is_empty(),
            "the child saw the name: {printed:?}"
        );
        std::env::remove_var("BISA_TEST_PROXY_NAME");
    }
}
