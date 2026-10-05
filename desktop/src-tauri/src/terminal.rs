//! An embedded terminal: one PTY per panel, owned by the desktop shell.
//!
//! # Why this lives in Tauri and not in the node
//!
//! **A shell is a capability of this machine, not a fact about the
//! workspace.** Everything else the desktop shows — goals, projects,
//! workstreams, files — is workspace state, and workspace state belongs to the
//! node, which is why the rest of this app is a thin client over its HTTP API.
//! A PTY is not that. It is the ambient authority of whoever is sitting at
//! this keyboard, and it is not something the workspace has, knows about, or
//! can be asked for.
//!
//! The concrete failure this prevents: the node serves its API on a unix
//! socket *and*, optionally, on a loopback TCP port, behind **one bearer
//! token** kept in a file of the workspace (`run/token`). That is a
//! defensible trade for a local daemon whose surface is the workspace —
//! whatever can read the token's file can already read the workspace beside
//! it. Add `POST /terminal` to it and the blast radius becomes the machine:
//! anything on it that comes by the token — any program that can read the
//! workspace's folder — gets an interactive root-less-but-real shell as the
//! user running the node.
//!
//! Behind Tauri's IPC the same PTY is reachable only from the app's own
//! webview, which loads only content this binary shipped. The capability stops
//! at the process boundary instead of at a port one secret is guarding.
//!
//! This is not a "for now" decision, and there is no version of it that gets
//! better with a stronger token: the token is readable by anything that can
//! already read the workspace. **The terminal must never become a node
//! route.**
//!
//! # What the frontend is and is not allowed to say
//!
//! The frontend never supplies a path. It names a `scope` and an `id`, and
//! this module asks the node `GET /placement/{scope}/{id}` for the absolute
//! directory. A UI that could pass an arbitrary cwd would hand the same
//! ambient authority back to whatever can reach the webview, and it would make
//! "where did this shell open?" a question with no authoritative answer. A
//! scope or id the node does not recognise is refused; it is never defaulted
//! to the home directory or the workspace root.
//!
//! **The same rule, applied to harnesses.** A terminal can run Claude Code,
//! Codex, opencode or anything else the catalog knows — and the frontend names
//! a *harness id*, never a program. `GET /harnesses` answers with the command,
//! and an id the node does not know, or one with no interactive form, is
//! refused. Accepting `{program, args}` from the webview would make this
//! module a general-purpose process launcher with a terminal attached, which
//! is precisely the authority the placement rule exists to withhold.
//!
//! **The frontend may read where a shell stands; it still names nothing.**
//! `terminal_cwd` answers the shell's current directory — the process table's
//! word for its pid, else the directory it was started in — so a relative
//! path the shell printed can be read from where the shell was (ide/17). That
//! is a fact read back, like a workstream's path: the webview resolves with
//! it on its own side and hands the node `(scope, id, relative)` as ever.
//!
//! Both lookups speak raw HTTP over a `TcpStream`, for the reason
//! `sidecar::wait_for_health` does: the desktop shell has no HTTP
//! client dependency, and two GETs against loopback do not justify adding
//! one.
//!
//! # Output
//!
//! Output streams over a [`tauri::ipc::Channel`] rather than the event bus.
//! Events are not ordered relative to each other; channels are, and a terminal
//! whose bytes arrive out of order is not a terminal. Tauri's own guidance
//! names streamed child-process output as the case channels exist for.

use crate::sidecar::NodeState;
use crate::sync::Locked;
use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::State;

/// Loopback, one small GET. Long enough to survive a node that is mid-restart,
/// short enough that a wedged node does not look like a hung app.
const PLACEMENT_TIMEOUT: Duration = Duration::from_secs(5);

/// One read from the master side. Larger than a keystroke by a wide margin so
/// a program that floods (a build log, `cat` of a big file) costs one wakeup
/// per 8KB rather than one per line.
const READ_BUF: usize = 8192;

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// What a terminal is rooted in: three of the scopes the node resolves a
/// directory for (`bisa_store::FileScope`'s goal, workstream and work item,
/// spelled the same way on the wire — a run of the workspace has no terminal)
/// and this machine.
///
/// Parsed here rather than passed through as a string, so a typo is refused at
/// the IPC boundary with a message naming the alternatives instead of becoming
/// a 400 from the node three layers down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Goal,
    /// A checkout — the project's own tree is its primary workstream (ADR-0031).
    Workstream,
    WorkItem,
    /// The person's home directory — a code host sign-in runs there, since it
    /// is about the machine and not a checkout. Nothing of the node's to ask.
    Machine,
}

impl Scope {
    const ALL: [Scope; 4] = [
        Scope::Goal,
        Scope::Workstream,
        Scope::WorkItem,
        Scope::Machine,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Scope::Machine => "machine",
            Scope::Goal => "goal",
            Scope::Workstream => "workstream",
            Scope::WorkItem => "work_item",
        }
    }

    /// Whether a harness opened here is a roster session: a row stands in
    /// something of the workspace's — the node places it by the same rule it
    /// places a terminal by — and the machine is nothing of the workspace's.
    /// A harness opened in the home folder is a terminal and no row, and the
    /// node is not asked for one it would refuse.
    fn has_a_roster_row(self) -> bool {
        self != Scope::Machine
    }

    fn parse(raw: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == raw)
            .ok_or_else(|| {
                let valid: Vec<&str> = Self::ALL.iter().map(|s| s.as_str()).collect();
                format!(
                    "unknown terminal scope {raw:?}: use one of {}",
                    valid.join(", ")
                )
            })
    }
}

// ---------------------------------------------------------------------------
// What a terminal says
// ---------------------------------------------------------------------------

/// Everything a session sends up the channel.
///
/// Tagged rather than two channels, because the frontend has to interleave the
/// two correctly — an `exit` that arrived before the last `output` would draw a
/// dead prompt above live text.
///
/// There is no error variant, and that is a claim rather than an omission: the
/// only way a running session ends is the shell going away. Everything that can
/// fail before that — an unknown scope, a directory that does not exist, a pty
/// that would not allocate — fails inside the command that was asked to do it
/// and comes back as a rejected `invoke`, where the caller still has the
/// context to say what it was trying to open.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TerminalEvent {
    /// A decoded run of bytes from the shell. Never a partial UTF-8 sequence:
    /// see [`Utf8Chunks`].
    Output { data: String },
    /// The shell is gone. `code` is `None` only when the wait itself failed.
    Exit { code: Option<i32> },
    /// What runs in this shell changed: a harness the catalog names started
    /// under it (`Some(id)`) or the last one ended (`None`) — the process
    /// table's word (`watch_processes`), never the bytes'. On the same
    /// ordered channel as the output, so it never arrives after the exit.
    Process { harness: Option<String> },
}

/// Where a session's events go.
///
/// A closure rather than the `Channel` itself so the PTY plumbing below has
/// no opinion about Tauri: the tests drive a real shell through a sink that
/// appends to a `String`, which is what makes the `pwd` round-trip testable
/// without a window. Shared (`Arc`) between the pump, which owns the output,
/// and the registry, which says what runs. Returns `false` once the far side
/// is gone.
type Sink = Arc<dyn Fn(TerminalEvent) -> bool + Send + Sync + 'static>;

// ---------------------------------------------------------------------------
// Decoding the master side
// ---------------------------------------------------------------------------

/// Reassembles UTF-8 across read boundaries.
///
/// A PTY read stops wherever the kernel's buffer stopped, which is regularly
/// in the middle of a multi-byte character — accented text, a box-drawing
/// glyph, any emoji a modern CLI prints. Decoding each read on its own turns
/// that into a pair of replacement characters that never heal, because the
/// second half is decoded as garbage too. Holding the incomplete tail until
/// its continuation bytes arrive is the only way the character survives.
#[derive(Default)]
struct Utf8Chunks {
    tail: Vec<u8>,
}

impl Utf8Chunks {
    /// Everything that is now completely decodable, with genuinely invalid
    /// bytes replaced rather than dropped — a terminal that silently swallows
    /// bytes hides the corruption that caused them.
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.tail.extend_from_slice(bytes);
        let mut out = String::with_capacity(self.tail.len());
        let mut from = 0usize;
        loop {
            match std::str::from_utf8(&self.tail[from..]) {
                Ok(s) => {
                    out.push_str(s);
                    self.tail.clear();
                    return out;
                }
                Err(e) => {
                    let good = e.valid_up_to();
                    // Safe: `valid_up_to` is by definition a UTF-8 boundary.
                    out.push_str(std::str::from_utf8(&self.tail[from..from + good]).unwrap());
                    match e.error_len() {
                        // Truncated at the end of what we have — keep it for
                        // the next read, which is the whole point of this type.
                        None => {
                            self.tail.drain(..from + good);
                            return out;
                        }
                        Some(bad) => {
                            out.push('\u{FFFD}');
                            from += good + bad;
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Asking the node where a scope lives
// ---------------------------------------------------------------------------

/// The body of `GET /placement/{scope}/{id}`.
#[derive(Deserialize)]
struct Placement {
    path: String,
    exists: bool,
}

/// `http://host:port/...` → the pair a `TcpStream` connects to.
///
/// Only plain `http://` is accepted. `BISA_API_BASE` can point the app at
/// an already-running node, and if someone points it somewhere this function
/// cannot honestly reach, the terminal has to say so — silently rewriting
/// `https://` to a plain connection would be worse than refusing.
pub(crate) fn authority(api_base: &str) -> Result<(String, u16), String> {
    let rest = api_base.strip_prefix("http://").ok_or_else(|| {
        format!("the terminal reaches the node over plain loopback HTTP only, but the API base is {api_base:?}")
    })?;
    let hostport = rest.split('/').next().unwrap_or_default();
    if hostport.contains('@') {
        return Err(format!("unsupported userinfo in the API base {api_base:?}"));
    }
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (
            h,
            p.parse::<u16>()
                .map_err(|_| format!("unintelligible port in the API base {api_base:?}"))?,
        ),
        None => (hostport, 80),
    };
    if host.is_empty() {
        return Err(format!("no host in the API base {api_base:?}"));
    }
    Ok((host.to_string(), port))
}

/// Percent-encode one path segment down to the unreserved set.
///
/// Not politeness. The request below is assembled by hand, so a `\r\n` inside
/// an id would let the caller append headers — or a whole second request — to
/// it. Encoding everything outside `A-Za-z0-9-._~` leaves nothing that can end
/// a request line, and it also means an id containing `/` asks about that id
/// rather than about some other route.
fn encode_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                write!(out, "%{b:02X}").expect("a String accepts every write");
            }
        }
    }
    out
}

/// Status code and body of a response read whole off a `Connection: close`
/// socket.
///
/// A chunked body is refused rather than half-parsed: the node answers this
/// route with a serialized `Json`, which always carries a content-length, so
/// chunking here would mean the response came from something that is not the
/// node — and guessing at that is how a minimal client starts smuggling.
pub(crate) fn parse_http_response(raw: &str) -> Result<(u16, String), String> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or("the node's answer ended before its headers did")?;
    let status_line = head.lines().next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| format!("unintelligible status line from the node: {status_line:?}"))?;
    let chunked = head.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
        })
    });
    if chunked {
        return Err(
            "the node answered with a chunked body, which this minimal client does not read".into(),
        );
    }
    Ok((status, body.to_string()))
}

/// One GET, whole response, no keep-alive.
pub(crate) fn get(host: &str, port: u16, token: &str, path: &str) -> Result<String, String> {
    let mut sock = TcpStream::connect((host, port))
        .map_err(|e| format!("the node is not reachable at {host}:{port}: {e}"))?;
    // A socket that cannot be bounded is not used: a node that never answers
    // would otherwise hold the shell's thread.
    sock.set_read_timeout(Some(PLACEMENT_TIMEOUT))
        .map_err(|e| format!("the node socket refused a timeout: {e}"))?;
    sock.set_write_timeout(Some(PLACEMENT_TIMEOUT))
        .map_err(|e| format!("the node socket refused a timeout: {e}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    sock.write_all(req.as_bytes())
        .map_err(|e| format!("could not ask the node where {path} lives: {e}"))?;
    let mut buf = Vec::new();
    sock.read_to_end(&mut buf)
        .map_err(|e| format!("the node stopped answering: {e}"))?;
    String::from_utf8(buf).map_err(|_| "the node's answer was not UTF-8".to_string())
}

/// One POST with a JSON body, whole response, no keep-alive. `token` is
/// whatever credential the route takes — the control-plane token, or an
/// interactive session's own secret.
fn post(host: &str, port: u16, token: &str, path: &str, body: &str) -> Result<String, String> {
    let mut sock = TcpStream::connect((host, port))
        .map_err(|e| format!("the node is not reachable at {host}:{port}: {e}"))?;
    // A socket that cannot be bounded is not used: a node that never answers
    // would otherwise hold the shell's thread.
    sock.set_read_timeout(Some(PLACEMENT_TIMEOUT))
        .map_err(|e| format!("the node socket refused a timeout: {e}"))?;
    sock.set_write_timeout(Some(PLACEMENT_TIMEOUT))
        .map_err(|e| format!("the node socket refused a timeout: {e}"))?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(req.as_bytes())
        .map_err(|e| format!("could not send {path} to the node: {e}"))?;
    let mut buf = Vec::new();
    sock.read_to_end(&mut buf)
        .map_err(|e| format!("the node stopped answering: {e}"))?;
    String::from_utf8(buf).map_err(|_| "the node's answer was not UTF-8".to_string())
}

/// The node's `{"error": "..."}`, or the bare status when it did not send one.
fn node_error(status: u16, body: &str) -> String {
    #[derive(Deserialize)]
    struct ErrBody {
        error: String,
    }
    match serde_json::from_str::<ErrBody>(body) {
        Ok(e) => e.error,
        Err(_) => format!("the node answered {status}"),
    }
}

// ---------------------------------------------------------------------------
// Asking the node how to run a harness
// ---------------------------------------------------------------------------

/// One row of `GET /harnesses`, cut down to what a terminal needs.
#[derive(Deserialize)]
struct HarnessRow {
    id: String,
    label: String,
    installed: bool,
    #[serde(default)]
    launch: Option<Launch>,
}

/// The command that runs a harness interactively, as the node reports it.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Launch {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// What continues this harness's latest session in the directory it starts
    /// in. Empty for every harness with no such form, which is why nothing
    /// here has to ask whether resuming is possible: appending nothing is
    /// opening fresh.
    #[serde(default)]
    pub resume_args: Vec<String>,
}

#[derive(Deserialize)]
struct HarnessList {
    harnesses: Vec<HarnessRow>,
}

/// What to run for a harness id, or a refusal that names the alternatives.
///
/// Three refusals, and they are three different things a person can fix:
/// an id nothing in the catalog has, an id whose harness has no interactive
/// form (every `acp:*`, every `a2a:*` — their command speaks a protocol), and
/// one the node cannot find on `PATH`. Collapsing them into "cannot open a
/// terminal" would leave somebody reinstalling a tool that was never the
/// problem.
///
/// A harness that probed as *not installed* is still refused here rather than
/// attempted. The alternative is a terminal that opens, prints `command not
/// found`, and exits — which reads as this feature being broken rather than as
/// the tool being absent.
fn resolve_launch(api_base: &str, token: &str, harness: &str) -> Result<Launch, String> {
    let (host, port) = authority(api_base)?;
    let (status, body) = parse_http_response(&get(&host, port, token, "/harnesses")?)?;
    if status != 200 {
        return Err(node_error(status, &body));
    }
    let list: HarnessList = serde_json::from_str(&body)
        .map_err(|e| format!("the node's harness list did not parse: {e}"))?;

    let Some(row) = list.harnesses.into_iter().find(|h| h.id == harness) else {
        return Err(format!("no harness {harness:?} in this workspace"));
    };
    let Some(launch) = row.launch else {
        return Err(format!(
            "{} has no interactive form — its command speaks a protocol rather than \
             running a session you can sit in front of",
            row.label
        ));
    };
    if !row.installed {
        return Err(format!(
            "{} is not installed on this machine, so there is nothing to run",
            row.label
        ));
    }
    Ok(launch)
}

/// The project's run command for a checkout, as the node answers it
/// (`GET /workstreams/{wid}/run-command`, ide/18): the text is a project
/// setting approved on this machine; the shell runs it through the login
/// shell like a harness, in the checkout. A command this machine has not
/// approved is refused with where to approve it; a scope that is not a
/// checkout has none.
fn run_launch(api_base: &str, token: &str, scope: Scope, id: &str) -> Result<Launch, String> {
    if scope != Scope::Workstream {
        return Err("a run command belongs to a checkout; open one there".to_string());
    }
    let (host, port) = authority(api_base)?;
    let (status, body) = parse_http_response(&get(
        &host,
        port,
        token,
        &format!("/workstreams/{id}/run-command"),
    )?)?;
    if status == 404 {
        return Err(
            "the project sets no run command — set one under About › Settings › Workstream scripts, or serve the branch from the Browser menu"
                .to_string(),
        );
    }
    if status != 200 {
        return Err(node_error(status, &body));
    }
    #[derive(Deserialize)]
    struct RunCommand {
        command: String,
        trusted: bool,
    }
    let run: RunCommand = serde_json::from_str(&body)
        .map_err(|e| format!("the node's run command did not parse: {e}"))?;
    if !run.trusted {
        return Err(
            "the run command is not approved on this machine — review and approve it under About › Settings › Workstream scripts"
                .to_string(),
        );
    }
    // `sh -c <text>` through the login shell: the text is the author's,
    // verbatim, one word to the shell that runs it — as the engine's own
    // script runner spells it.
    Ok(Launch {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), run.command],
        resume_args: Vec::new(),
    })
}

/// A Flutter app to run on a device (ide/19): the webview names the device's
/// id, and nothing else.
#[derive(Debug, Clone, Deserialize)]
pub struct MobileDevelopmentSpec {
    pub device: String,
}

/// The `flutter run` line for a checkout and a device, as the node answers
/// it (`GET /workstreams/{wid}/mobile-development/run-command?device=`, ide/19): the
/// resolved `flutter`, quoted, and the device — composed by the engine, run
/// here through the login shell like the project's run command. A node that
/// says mobile development is off, or has no Flutter, is the answer.
fn mobile_development_launch(
    api_base: &str,
    token: &str,
    scope: Scope,
    id: &str,
    device: &str,
) -> Result<(Launch, Option<String>), String> {
    if scope != Scope::Workstream {
        return Err("an app runs from a checkout; open one there".to_string());
    }
    let (host, port) = authority(api_base)?;
    let (status, body) = parse_http_response(&get(
        &host,
        port,
        token,
        &format!(
            "/workstreams/{}/mobile-development/run-command?device={}",
            encode_segment(id),
            encode_segment(device)
        ),
    )?)?;
    if status == 404 {
        return Err(
            "mobile development is off, or this checkout has no Flutter app — Settings › Capabilities › Mobile Development"
                .to_string(),
        );
    }
    if status != 200 {
        return Err(node_error(status, &body));
    }
    #[derive(Deserialize)]
    struct RunCommand {
        command: String,
        #[serde(default)]
        cwd: Option<String>,
    }
    let run: RunCommand = serde_json::from_str(&body)
        .map_err(|e| format!("the node's run line did not parse: {e}"))?;
    Ok((
        Launch {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), run.command],
            resume_args: Vec::new(),
        },
        run.cwd,
    ))
}

/// The folder a mobile run opens in: the checkout the placement named, or
/// the folder the node named when it is under that checkout — never one
/// outside it, whatever the node said.
fn mobile_development_cwd(root: &Path, cwd: Option<String>) -> Result<PathBuf, String> {
    let Some(cwd) = cwd.map(PathBuf::from) else {
        return Ok(root.to_path_buf());
    };
    if !cwd.starts_with(root)
        || cwd
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("the node named a folder outside the checkout".to_string());
    }
    if !cwd.is_dir() {
        return Err(format!("{} is not a folder on this machine", cwd.display()));
    }
    Ok(cwd)
}

/// The absolute directory a shell for this scope and id should open in.
///
/// `exists: false` is refused rather than created. A directory the workspace
/// has not laid down yet is not this module's to invent, and a shell that
/// silently opened somewhere else would be a worse answer than none.
fn resolve_dir(api_base: &str, token: &str, scope: Scope, id: &str) -> Result<PathBuf, String> {
    if matches!(scope, Scope::Machine) {
        // The home directory, from the environment the shell itself would
        // read; the node is not asked — a machine has no placement.
        return std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .ok_or_else(|| "no home directory to open a shell in".to_string());
    }
    let (host, port) = authority(api_base)?;
    let path = format!("/placement/{}/{}", scope.as_str(), encode_segment(id));
    let (status, body) = parse_http_response(&get(&host, port, token, &path)?)?;
    if status != 200 {
        return Err(node_error(status, &body));
    }
    let placement: Placement = serde_json::from_str(&body)
        .map_err(|e| format!("the node's placement answer did not parse: {e}"))?;
    if !placement.exists {
        return Err(format!(
            "{} {id} has no directory yet ({}), so there is nothing to open a shell in",
            scope.as_str(),
            placement.path
        ));
    }
    Ok(PathBuf::from(placement.path))
}

// ---------------------------------------------------------------------------
// Registering a harness as a roster session
// ---------------------------------------------------------------------------

/// What `POST /sessions/terminal` answers: the session and the plan applied
/// to the command — the environment (the session's secret among it), the
/// arguments — or no session, when the harness cannot report and opens as a
/// plain terminal.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Interactive {
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Names the command must not inherit — the proxy variables the node's
    /// `network.proxy.mode` says the harness is not handed.
    #[serde(default)]
    pub env_remove: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub intercept_approval_notifications: bool,
}

/// The name the harness — and every hook it runs — finds the node under.
const ENV_NODE_URL: &str = "BISA_NODE_URL";
/// The variable the node put the session's secret in — the one place it is.
const ENV_SECRET: &str = "BISA_SESSION_SECRET";
/// What no PTY inherits from the app: the control-plane token. The same
/// list the node strips from every harness child (`bisa-harness`'s
/// `SCRUBBED_ENV`), spelled here because the shell does not link that crate.
const SCRUBBED_ENV: &[&str] = &["BISA_API_TOKEN"];

/// Register the harness with the node and get the plan that makes it report.
/// A node that refuses is a refusal here: a harness that cannot be registered
/// still opens, but as a terminal the roster does not know.
fn open_interactive(
    api_base: &str,
    token: &str,
    scope: Scope,
    id: &str,
    harness: &str,
) -> Result<Interactive, String> {
    let (host, port) = authority(api_base)?;
    let body =
        serde_json::json!({ "scope": scope.as_str(), "id": id, "harness": harness }).to_string();
    let (status, body) =
        parse_http_response(&post(&host, port, token, "/sessions/terminal", &body)?)?;
    if status != 200 {
        return Err(node_error(status, &body));
    }
    serde_json::from_str(&body).map_err(|e| format!("the node's session answer did not parse: {e}"))
}

/// A live session's link to its roster row: where to report, and as whom.
#[derive(Clone, Debug)]
struct Reporter {
    api_base: String,
    session: String,
    secret: String,
    /// Watch the harness's own terminal notifications (OSC 9) for an
    /// approval prompt and report it — Codex's trust-free signal.
    intercept_approvals: bool,
}

impl Reporter {
    /// The reporter a registered harness reports through: the session and,
    /// from the environment the node handed back, its secret.
    fn from_plan(plan: &Interactive, api_base: &str) -> Option<Self> {
        Some(Self {
            api_base: api_base.to_string(),
            session: plan.session.clone()?,
            secret: plan.env.get(ENV_SECRET)?.clone(),
            intercept_approvals: plan.intercept_approval_notifications,
        })
    }

    fn send(&self, path: &str, body: &str) {
        let (host, port) = match authority(&self.api_base) {
            Ok(at) => at,
            Err(e) => {
                tracing::warn!(target: "bisa_desktop", session = %self.session, door = path, "a session report has nowhere to go: {e}");
                return;
            }
        };
        // Best effort: a report the node did not take is a row that reads a
        // beat late, never a terminal that misbehaves. Said in the log, since
        // a row that reads *running* after its process ended has no other
        // line to explain it — the session and the door, never the body and
        // never the secret.
        if let Err(e) = post(
            &host,
            port,
            &self.secret,
            &format!("/sessions/{}/{path}", self.session),
            body,
        ) {
            tracing::debug!(target: "bisa_desktop", session = %self.session, door = path, "the node did not take a session report: {e}");
        }
    }

    /// The login shell is up: its pid, so the row can be traced to its
    /// process — and so the node's sweep can tell when that process is gone
    /// with nobody here to say so. The engine's own `process_started` event,
    /// through the report door.
    fn process_started(&self, pid: u32) {
        let event = serde_json::json!({ "tier": "lifecycle", "event": { "type": "process_started", "pid": pid } });
        self.send(
            "report",
            &serde_json::json!({ "events": [event] }).to_string(),
        );
    }

    /// The process ended by itself: its status, or the signal that ended it.
    fn exited(&self, code: Option<i32>, signal: Option<&str>) {
        self.send(
            "exit",
            &serde_json::json!({ "code": code, "signal": signal }).to_string(),
        );
    }

    /// The tab is gone: the row leaves the roster now, whatever the process
    /// was doing.
    fn closed(&self) {
        self.send("close", "{}");
    }

    /// The person answered the harness's dialog in this tab: no hook says
    /// so, the tab does — the row's wait is over.
    fn answered(&self) {
        self.send("answered", "{}");
    }

    /// The approval prompt Codex announced through the terminal, as the
    /// engine's own event — the same shape a hook would report.
    fn approval_requested(&self) {
        let event = serde_json::json!({
            "tier": "lifecycle",
            "event": {
                "type": "input_requested",
                "request": { "id": "approval", "kind": "permission", "tool_name": "approval", "tier": "exec", "args_summary": "" }
            }
        });
        self.send(
            "report",
            &serde_json::json!({ "events": [event] }).to_string(),
        );
    }
}

/// Finds OSC 9 notifications — `ESC ] 9 ; <text> BEL` or `… ESC \` — in the
/// bytes a shell prints, across read boundaries. The bytes themselves still
/// reach the emulator untouched; this only reads over its shoulder.
#[derive(Default)]
struct Osc9Scanner {
    /// A notification that started in an earlier chunk and has not ended.
    pending: Option<String>,
}

impl Osc9Scanner {
    const START: &'static str = "\x1b]9;";

    fn feed(&mut self, text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = text;
        loop {
            if let Some(mut open) = self.pending.take() {
                match Self::terminator(rest) {
                    Some((end, len)) => {
                        open.push_str(&rest[..end]);
                        found.push(open);
                        rest = &rest[end + len..];
                    }
                    None => {
                        open.push_str(rest);
                        // A notification is one line; anything longer is not
                        // one, and is dropped rather than buffered forever.
                        if open.len() <= 512 {
                            self.pending = Some(open);
                        }
                        return found;
                    }
                }
            }
            let Some(start) = rest.find(Self::START) else {
                return found;
            };
            self.pending = Some(String::new());
            rest = &rest[start + Self::START.len()..];
        }
    }

    /// Where the open notification ends, and how long the terminator is.
    fn terminator(s: &str) -> Option<(usize, usize)> {
        let bel = s.find('\x07').map(|i| (i, 1));
        let st = s.find("\x1b\\").map(|i| (i, 2));
        match (bel, st) {
            (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
            (a, b) => a.or(b),
        }
    }
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// Where a tab's process is in its life. **The tab is the row**: the entry
/// lives as long as the tab, not as long as the process, so a harness that
/// exited on its own still has a reporter to say *close* with when its tab
/// finally goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The process runs; the pump reads it.
    Live,
    /// `terminate` asked it to end; the pump will report *close*, not *exit*.
    Closing,
    /// The process ended by itself and said so; the tab is still open.
    Exited,
}

/// One tab's shell.
///
/// The `Child` itself is not here — it belongs to the reader thread, which is
/// the only thing that can meaningfully wait on it. What stays behind is a
/// cloned killer, which is exactly what `portable_pty::ChildKiller::clone_killer`
/// exists for: closing a terminal must not have to reach across a thread that
/// is blocked in `read`. The master, the writer and the killer go with the
/// process; the reporter and the pid stay until the tab does.
struct Session {
    phase: Phase,
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    /// The login shell's OS pid, when the platform reports one — the root the
    /// port scanner walks up to, and what the escalation signals. A server
    /// the shell starts is a descendant of this pid.
    pid: Option<u32>,
    /// Where the shell was started — the placement the node named (a mobile
    /// run's folder under it). What `terminal_cwd` answers once the process
    /// is gone, or on a platform whose process table has no cwd to give.
    dir: PathBuf,
    /// The roster row this tab is, when the harness in it reports.
    reporter: Option<Reporter>,
    /// The tab's channel, shared with the pump, so what runs in the shell
    /// can be said between the bytes. Goes with the process.
    sink: Option<Sink>,
    /// The harness the process table last showed under this shell — what
    /// the webview was told; a change is one `Process` event.
    running: Option<String>,
}

/// How long a process that ignored `SIGHUP` gets before `SIGTERM`, and how
/// long after that before `SIGKILL`. A harness that traps the hang-up to save
/// its state has a quarter second to finish; one that ignores everything is
/// ended in under a second, so a tab never closes on a process that lives on.
const TERMINATE_GRACE: Duration = Duration::from_millis(250);
const TERMINATE_LAST_WORD: Duration = Duration::from_millis(500);
/// How long a shutdown waits for every pump to report its close before the
/// process goes: the node hears *close* for each tab, or the app leaves anyway.
const SHUTDOWN_CAP: Duration = Duration::from_millis(1500);

/// Every shell this app has open.
#[derive(Default)]
pub struct TerminalRegistry {
    sessions: Mutex<HashMap<String, Session>>,
    next: AtomicU64,
    /// The window is gone: nothing may be added, and anything that arrives
    /// late is ended at once.
    closed: AtomicBool,
    /// The process watcher is running; it stops itself once nothing is live.
    watching: AtomicBool,
}

impl TerminalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ids are per-process counters, not random. The map they key is
    /// process-local and dies with the app, so uniqueness within this run is
    /// the whole requirement — and a readable id makes a leaked session
    /// obvious in a log instead of being one more opaque blob.
    fn next_id(&self) -> String {
        format!("term-{}", self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }

    /// Take a freshly spawned shell in. `false` once the window is gone: the
    /// caller then ends what it just started rather than leaving a shell
    /// nobody can see.
    fn insert(&self, id: String, session: Session) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.sessions.locked().insert(id, session);
        true
    }

    fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let mut sessions = self.sessions.locked();
        let session = sessions.get_mut(id).ok_or_else(|| unknown(id))?;
        let writer = session.writer.as_mut().ok_or_else(|| exited(id))?;
        writer
            .write_all(data.as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|e| format!("{id} stopped accepting input: {e}"))
    }

    fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        let sessions = self.sessions.locked();
        let session = sessions.get(id).ok_or_else(|| unknown(id))?;
        let master = session.master.as_ref().ok_or_else(|| exited(id))?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("could not resize {id}: {e}"))
    }

    /// Where a shell stands now: its process's current directory, else the
    /// directory it was started in — a dead tab's scrollback still reads its
    /// paths from where the shell began. The process table is asked outside
    /// the lock; it is a syscall, not a map read.
    fn cwd(&self, id: &str) -> Result<String, String> {
        let (pid, dir) = {
            let sessions = self.sessions.locked();
            let session = sessions.get(id).ok_or_else(|| unknown(id))?;
            (session.pid, session.dir.clone())
        };
        Ok(place_of(pid.and_then(cwd_of), &dir).display().to_string())
    }

    /// What the process table showed under a shell. A change is told to the
    /// tab on its own channel; the same answer twice says nothing.
    fn note_running(&self, id: &str, harness: Option<String>) {
        let sink = {
            let mut sessions = self.sessions.locked();
            let Some(session) = sessions.get_mut(id) else {
                return;
            };
            if session.phase != Phase::Live || session.running == harness {
                return;
            }
            session.running = harness.clone();
            session.sink.clone()
        };
        if let Some(sink) = sink {
            let _ = sink(TerminalEvent::Process { harness });
        }
    }

    /// Start the process watcher unless it is already running. It reads the
    /// machine's process table every `WATCH_EVERY` while any shell is live
    /// (the port scanner's own source, ide/01: the machine's, so the shell's)
    /// and tells each tab which harness runs under it.
    fn ensure_watching(self: &Arc<Self>, api_base: String, token: String) {
        if self.watching.swap(true, Ordering::SeqCst) {
            return;
        }
        let registry = Arc::clone(self);
        if std::thread::Builder::new()
            .name("pty-watch".into())
            .spawn(move || watch_processes(registry, &api_base, &token))
            .is_err()
        {
            self.watching.store(false, Ordering::SeqCst);
        }
    }

    /// The pid behind every live terminal, for the port scanner.
    pub fn pids(&self) -> Vec<(String, u32)> {
        self.sessions
            .locked()
            .iter()
            .filter(|(_, s)| s.phase == Phase::Live)
            .filter_map(|(id, s)| s.pid.map(|pid| (id.clone(), pid)))
            .collect()
    }

    /// The tab is closing: end its process and, through it, its roster row.
    ///
    /// A live process is asked to hang up now and pressed harder on a clock
    /// (`TERMINATE_GRACE`, `TERMINATE_LAST_WORD`); the pump, which alone
    /// waits on the child, reports *close* when it ends. A process that
    /// already exited has only its row left to close, and that is posted
    /// here — off this thread, since it is a request to the node.
    ///
    /// Deliberately not an error when the id is unknown. The frontend closes
    /// on unmount, and a tab the app already shut down is gone by then —
    /// turning that race into a rejected promise would teach the UI to
    /// swallow close failures, which is how a leaked shell comes back.
    fn terminate(self: &Arc<Self>, id: &str) {
        let posting = {
            let mut sessions = self.sessions.locked();
            let Some(session) = sessions.get_mut(id) else {
                return;
            };
            match session.phase {
                Phase::Closing => return,
                Phase::Exited => sessions.remove(id).and_then(|s| s.reporter),
                Phase::Live => {
                    // Marked before the signal, under the lock: the pump can
                    // read nothing but `Closing` once the process ends.
                    session.phase = Phase::Closing;
                    if let Some(killer) = session.killer.as_mut() {
                        if let Err(e) = killer.kill() {
                            tracing::debug!("the shell had already ended: {e}");
                        }
                    }
                    if let Some(pid) = session.pid {
                        self.escalate(id.to_string(), pid);
                    }
                    None
                }
            }
        };
        if let Some(reporter) = posting {
            std::thread::spawn(move || reporter.closed());
        }
    }

    /// `SIGTERM`, then `SIGKILL`, to a process that did not take the hang-up
    /// — each only while the tab is still closing, so a process that ended
    /// in time is never signalled twice.
    fn escalate(self: &Arc<Self>, id: String, pid: u32) {
        let registry = Arc::clone(self);
        std::thread::spawn(move || {
            for (wait, signal) in [
                (TERMINATE_GRACE, sysinfo::Signal::Term),
                (TERMINATE_LAST_WORD, sysinfo::Signal::Kill),
            ] {
                std::thread::sleep(wait);
                if !registry.is_closing(&id) {
                    return;
                }
                crate::ports::signal(pid, signal);
            }
        });
    }

    /// Codex announced an approval prompt through the terminal: report it as
    /// the engine's own event, through the tab's reporter.
    fn approval_requested(&self, id: &str) {
        let reporter = self
            .sessions
            .locked()
            .get(id)
            .and_then(|s| s.reporter.clone());
        if let Some(reporter) = reporter {
            reporter.approval_requested();
        }
    }

    /// The person answered in the tab: told to the node off the lock, like
    /// every other word of the reporter's. A tab that is no roster row has
    /// nobody to tell.
    fn answered(&self, id: &str) {
        let reporter = self
            .sessions
            .locked()
            .get(id)
            .and_then(|s| s.reporter.clone());
        if let Some(reporter) = reporter {
            reporter.answered();
        }
    }

    fn is_closing(&self, id: &str) -> bool {
        self.sessions
            .locked()
            .get(id)
            .is_some_and(|s| s.phase == Phase::Closing)
    }

    /// The pump's last act: the process ended. Says what it means to the
    /// roster — *close* for a tab that was closing (or a webview that is
    /// gone), *exit* for a process that ended by itself — and keeps the
    /// entry as `Exited` for the latter, so the tab still owns a reporter.
    fn finished(&self, id: &str, code: Option<i32>, signal: Option<&str>, connected: bool) {
        let (phase, reporter) = {
            let mut sessions = self.sessions.locked();
            let Some(session) = sessions.get_mut(id) else {
                return;
            };
            let closing = session.phase == Phase::Closing || !connected;
            if closing {
                (Phase::Closing, sessions.remove(id).and_then(|s| s.reporter))
            } else {
                session.phase = Phase::Exited;
                session.master = None;
                session.writer = None;
                session.killer = None;
                session.sink = None;
                session.running = None;
                (Phase::Exited, session.reporter.clone())
            }
        };
        if let Some(reporter) = reporter {
            match phase {
                Phase::Closing => reporter.closed(),
                _ => reporter.exited(code, signal),
            }
        }
    }

    /// A shell that was spawned but never pumped: ended and dropped, with no
    /// word to the roster — the caller says *close* itself.
    fn abandon(&self, id: &str) {
        if let Some(mut session) = self.sessions.locked().remove(id) {
            if let Some(killer) = session.killer.as_mut() {
                if let Err(e) = killer.kill() {
                    tracing::debug!("the shell had already ended: {e}");
                }
            }
        }
    }

    /// Every tab, closed. Called on window destruction and on app exit,
    /// mirroring `NodeState::shutdown`: a shell that outlives the window that
    /// opened it has no way to be closed and no one watching it. Waits, up to
    /// `SHUTDOWN_CAP`, for every pump to have told the roster — so the rows
    /// go with the window rather than lingering as orphans.
    pub fn shutdown_all(self: &Arc<Self>) {
        self.closed.store(true, Ordering::SeqCst);
        let ids: Vec<String> = self.sessions.locked().keys().cloned().collect();
        for id in &ids {
            self.terminate(id);
        }
        let until = Instant::now() + SHUTDOWN_CAP;
        while Instant::now() < until {
            if self
                .sessions
                .locked()
                .values()
                .all(|s| s.phase != Phase::Closing)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }
}

fn unknown(id: &str) -> String {
    format!("no terminal {id:?} — it has already been closed")
}

fn exited(id: &str) -> String {
    format!("terminal {id:?} has exited; restart it to type into it")
}

// ---------------------------------------------------------------------------
// Where a shell stands
// ---------------------------------------------------------------------------

/// One process's current directory, as the OS reports it — the one pid is
/// refreshed, never the table: a click asks this, and a click must not cost a
/// scan. `None` for a process that is gone or a platform that does not say.
fn cwd_of(pid: u32) -> Option<PathBuf> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new().with_cwd(UpdateKind::Always),
    );
    sys.process(pid)
        .and_then(|p| p.cwd())
        .map(Path::to_path_buf)
}

/// Where a shell is read from: what the process table said, else where the
/// shell was started.
fn place_of(reported: Option<PathBuf>, started_in: &Path) -> PathBuf {
    reported.unwrap_or_else(|| started_in.to_path_buf())
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// One word of a `sh -c` command line, quoted so it stays one word.
///
/// **This is a boundary, not politeness.** `args` reaches here from a tier-3
/// custom harness descriptor — a JSON file the user wrote — by way of the
/// node's catalog, and it is about to be interpolated into a string a shell
/// will parse. Single quotes suspend every expansion a shell does, and the
/// only character they cannot contain is a single quote, which is why the
/// escape is `'\''`: end the quoted run, emit a literal quote, start a new one.
/// A semicolon, a `$(…)`, a backtick or a newline inside a word is then data.
///
/// Same reasoning as `bisa-vcs`'s `:(top,literal)` pathspecs, one layer
/// down: a name that becomes part of a command is data, not syntax.
fn shell_word(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', r"'\''"))
}

/// `exec <program> <args…>`, safe to hand to `sh -lc`.
///
/// `exec` rather than a plain call so the harness *replaces* the shell instead
/// of running under it. One process per PTY, so "the shell exited" keeps
/// meaning exactly what it meant when the shell was the only thing here, and
/// closing the tab kills the thing you were looking at rather than a parent
/// that would outlive it.
fn shell_command(launch: &Launch, resume: bool, extra: &[String]) -> String {
    let mut out = format!("exec {}", shell_word(&launch.program));
    for arg in &launch.args {
        out.push(' ');
        out.push_str(&shell_word(arg));
    }
    // Quoted like every other word, and for the same reason: these reach here
    // from a tier-3 descriptor the user wrote, by way of the node, into a
    // string a shell is about to parse.
    if resume {
        for arg in &launch.resume_args {
            out.push(' ');
            out.push_str(&shell_word(arg));
        }
    }
    // The reporting plan's words come last: they name files the node wrote
    // and flags the harness reads after its own, and they are data too.
    for arg in extra {
        out.push(' ');
        out.push_str(&shell_word(arg));
    }
    out
}

/// The command a session runs.
///
/// Its own function because of the bug it exists to make impossible.
/// `CommandBuilder::arg` **panics** on a builder from `new_default_prog` —
/// `is_default_prog()` is just "argv is empty", and appending to it would leave
/// the shell with no program to be. Production is the only caller that passes
/// `program: None`, and every test passed `Some("/bin/sh")`, so the one
/// combination that shipped — the login shell *plus* a harness to run in it —
/// was the one combination never constructed. Opening any harness panicked.
///
/// So the shell is named rather than left implicit. `get_shell()` is
/// portable-pty's own resolution — `$SHELL` when it is executable, the passwd
/// entry otherwise — which is what `new_default_prog` would have resolved to,
/// so a harness gets the same shell the plain terminal opens.
///
/// Naming it costs the `-` argv[0] prefix that makes a login shell. That is
/// what the explicit `-l` is for, and always was.
fn build_command(
    program: Option<&str>,
    launch: Option<&Launch>,
    resume: bool,
    extra: &[String],
) -> CommandBuilder {
    let mut cmd = match program {
        None => CommandBuilder::new_default_prog(),
        Some(p) => CommandBuilder::new(p),
    };
    if let Some(launch) = launch {
        if cmd.is_default_prog() {
            cmd = CommandBuilder::new(cmd.get_shell());
        }
        cmd.args(["-l", "-c", &shell_command(launch, resume, extra)]);
    }
    cmd
}

/// Open a PTY in `dir` and start pumping it into `sink`.
///
/// `program` is `None` everywhere except in tests, where it is `/bin/sh`: the
/// production path must be the user's real login shell, and a test that
/// depended on whatever a developer's `.zshrc` prints would fail for reasons
/// that have nothing to do with this code.
///
/// `launch`, when set, runs a harness **through that same login shell** rather
/// than spawning its binary directly. That is not indirection for its own
/// sake. A GUI-launched app inherits launchd's minimal `PATH`, where `claude`
/// installed by nvm, mise or Homebrew does not appear — and neither do the API
/// keys a person's profile exports. Running it the way they would run it
/// themselves is the only way it sees the same tools and the same environment,
/// and the plain-shell path already pays that cost for the same reason.
// A session is its directory, its program, its size and its sink; splitting
// those into a struct would name nothing the call site does not already say.
#[allow(clippy::too_many_arguments)]
fn spawn_session(
    registry: Arc<TerminalRegistry>,
    dir: &Path,
    program: Option<&str>,
    launch: Option<&Launch>,
    harness_id: Option<&str>,
    resume: bool,
    plan: Option<(&Interactive, &str)>,
    rows: u16,
    cols: u16,
    sink: Sink,
) -> Result<String, String> {
    // portable-pty falls back to `$HOME` when the cwd it was given is not a
    // directory. A shell that quietly opens in the wrong place while the panel
    // above it says "workstream X" is worse than one that refuses to open.
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let extra: Vec<String> = plan.map(|(p, _)| p.args.clone()).unwrap_or_default();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("could not allocate a pty: {e}"))?;

    // `new_default_prog` is the login shell: `$SHELL` when it is executable,
    // the passwd database otherwise, launched with argv[0] prefixed by `-` so
    // it reads the user's login profile. That is the point of an embedded
    // terminal — it has to be the same shell, with the same PATH and the same
    // tool versions, as the one the user would have opened themselves.
    // A harness runs *inside* that shell, so it inherits the same profile.
    // `-l` because a non-login shell reads a different (often much smaller)
    // set of startup files, and the whole reason for going through a shell at
    // all is the environment the login files build. See [`build_command`].
    let mut cmd = build_command(program, launch, resume, &extra);
    cmd.cwd(dir);
    // xterm.js is an xterm-256color emulator with truecolor support. Without
    // these two, programs downgrade to a colourless 8-colour terminal and the
    // panel renders monochrome output nobody can explain.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // So a shell profile, or a tool inside it, can tell it is running here.
    cmd.env("BISA_TERMINAL", "1");
    // The control-plane token is the app's authority, never a PTY's: the same
    // names the node strips from every harness child are stripped here, so
    // an app started from a shell that exported one passes it on to nobody.
    for name in SCRUBBED_ENV {
        cmd.env_remove(name);
    }
    // And which harness, if any — enough for a prompt or a status line to say
    // what this pane is, without having to guess from the process table.
    if let Some(id) = harness_id {
        cmd.env("BISA_HARNESS", id);
    }
    // A harness that reports gets its session, its secret and where the node
    // is — a capability for exactly one roster row. The control-plane token
    // is still never in a PTY.
    let reporter = plan.and_then(|(p, api_base)| {
        let reporter = Reporter::from_plan(p, api_base)?;
        for (k, v) in &p.env {
            cmd.env(k, v);
        }
        for name in &p.env_remove {
            cmd.env_remove(name);
        }
        cmd.env(ENV_NODE_URL, api_base);
        Some(reporter)
    });

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("could not start a shell in {}: {e}", dir.display()))?;

    // The slave fd must go now. While this process still holds one, the master
    // never reaches EOF, so a shell that exits leaves the reader thread blocked
    // forever and the session never reports its exit.
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("could not read from the pty: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("could not write to the pty: {e}"))?;
    let killer = child.clone_killer();
    // Before the child is moved into the pump thread: the login shell's pid,
    // which the port scanner walks descendants up to (ADR-0054) — and which
    // the roster row learns, off the spawn path, so a dead process can be
    // told from a quiet one.
    let pid = child.process_id();
    if let (Some(reporter), Some(pid)) = (reporter.clone(), pid) {
        std::thread::spawn(move || reporter.process_started(pid));
    }

    let id = registry.next_id();
    let mut killer_for_late = child.clone_killer();
    let accepted = registry.insert(
        id.clone(),
        Session {
            phase: Phase::Live,
            master: Some(pair.master),
            writer: Some(writer),
            killer: Some(killer),
            pid,
            dir: dir.to_path_buf(),
            reporter: reporter.clone(),
            sink: Some(Arc::clone(&sink)),
            running: None,
        },
    );
    if !accepted {
        // The window went while this shell was being spawned: end it now,
        // before anything reads it. The caller tells the roster.
        if let Err(e) = killer_for_late.kill() {
            tracing::debug!("the shell had already ended: {e}");
        }
        return Err("the app is shutting down".to_string());
    }

    let pump_id = id.clone();
    let pump_registry = Arc::clone(&registry);
    let scan_approvals = reporter.as_ref().is_some_and(|r| r.intercept_approvals);
    if let Err(e) = std::thread::Builder::new()
        .name(format!("pty-{id}"))
        .spawn(move || pump(reader, child, pump_id, pump_registry, sink, scan_approvals))
    {
        // Nothing is reading the shell, so nothing ever will. Take it back down
        // rather than leaving an orphan nobody can see; the caller tells the
        // roster.
        registry.abandon(&id);
        return Err(format!("could not start the reader for {id}: {e}"));
    }
    Ok(id)
}

/// Master side → sink, until one end stops. The one thread that waits on the
/// child, so the one that tells the roster how the process ended
/// ([`TerminalRegistry::finished`]).
fn pump(
    mut reader: Box<dyn Read + Send>,
    mut child: Box<dyn Child + Send + Sync>,
    id: String,
    registry: Arc<TerminalRegistry>,
    sink: Sink,
    scan_approvals: bool,
) {
    let mut buf = [0u8; READ_BUF];
    let mut chunks = Utf8Chunks::default();
    let mut connected = true;
    let mut notifications = scan_approvals.then(Osc9Scanner::default);
    loop {
        match reader.read(&mut buf) {
            // A signal landing mid-read is not the shell exiting. `File`'s
            // `Read` does not retry `EINTR` for us, and treating it as EOF
            // would tear down a perfectly live session whenever the process
            // happened to catch a signal.
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // Zero is EOF; every other error is how a pty reports that its
            // child is gone — EIO on Linux once the last slave fd closes,
            // plain EOF on macOS — so both mean the same thing here.
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let text = chunks.decode(&buf[..n]);
                if let Some(scanner) = notifications.as_mut() {
                    if scanner
                        .feed(&text)
                        .iter()
                        .any(|n| n.to_ascii_lowercase().contains("approval"))
                    {
                        registry.approval_requested(&id);
                    }
                }
                if !text.is_empty() && !sink(TerminalEvent::Output { data: text }) {
                    connected = false;
                    break;
                }
            }
        }
    }

    // The frontend went away mid-stream: the shell is now talking to nobody,
    // and waiting on it would park this thread until the machine reboots.
    if !connected {
        registry.terminate(&id);
    }

    let status = child.wait().ok();
    let signal = status.as_ref().and_then(|s| s.signal().map(str::to_string));
    // A signal death reads as status 1 beside its signal; the signal is the
    // fact, so the status is not reported as if the process had chosen it.
    let code = status
        .as_ref()
        .filter(|_| signal.is_none())
        .map(|s| s.exit_code() as i32);
    registry.finished(&id, code, signal.as_deref(), connected);
    let _ = sink(TerminalEvent::Exit { code });
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Open a shell — or a harness — in whatever directory the node says this
/// scope and id live in.
///
/// `harness` is an id from `GET /harnesses`, or `None` for the login shell. It
/// is never a program: see this module's doc for why the webview names things
/// and this side resolves them.
///
/// `resume` continues that harness's latest session in the resolved directory
/// rather than starting a new one. It is ignored for a plain shell, which has
/// no session to continue, and for a harness whose launch names no way to
/// resume — in both cases nothing is appended and the result is a fresh start.
///
/// `async` on purpose: a synchronous Tauri command runs on the main thread, and
/// the two lookups below are blocking socket round-trips. Doing them there
/// would freeze the window for as long as the node takes to answer.
#[tauri::command]
// A Tauri command's parameters are its IPC signature; the frontend names
// each one, and a struct here would be a second schema for the same call.
#[allow(clippy::too_many_arguments)]
pub async fn terminal_open(
    node: State<'_, NodeState>,
    registry: State<'_, Arc<TerminalRegistry>>,
    scope: String,
    id: String,
    harness: Option<String>,
    resume: bool,
    rows: u16,
    cols: u16,
    login: Option<LoginSpec>,
    run: bool,
    mobile_development: Option<MobileDevelopmentSpec>,
    on_output: Channel<TerminalEvent>,
) -> Result<TerminalOpened, String> {
    // No node, no shell: the reason is the answer.
    let api_base = node.api_base()?;
    let api_token = node.api_token();
    let registry = Arc::clone(registry.inner());
    tauri::async_runtime::spawn_blocking(move || {
        // The watcher's handle, taken before the spawn consumes the registry's.
        let watcher = Arc::clone(&registry);
        let scope = Scope::parse(&scope)?;
        let mut dir = resolve_dir(&api_base, &api_token, scope, &id)?;
        // A Flutter app on a device (ide/19): the node's line for this
        // checkout and this device, run in the folder it names under the
        // checkout — never a program the webview named.
        let mobile_development_run = match (&login, &mobile_development) {
            (None, Some(spec)) => {
                let (launch, cwd) = mobile_development_launch(&api_base, &api_token, scope, &id, &spec.device)?;
                dir = mobile_development_cwd(&dir, cwd)?;
                Some(launch)
            }
            _ => None,
        };
        let (launch, interactive) = match (login, harness.as_deref(), run) {
            _ if mobile_development_run.is_some() => (mobile_development_run, None),
            // A code host sign-in: the CLI's own browser login, from the
            // closed table below — never a program the webview named.
            (Some(spec), _, _) => (Some(login_launch(&spec)?), None),
            // The project's run command (ide/18): the node's answer for this
            // checkout, approved on this machine — never a program the
            // webview named.
            (None, _, true) => (Some(run_launch(&api_base, &api_token, scope, &id)?), None),
            (None, Some(h), false) => {
                let launch = resolve_launch(&api_base, &api_token, h)?;
                // Registered before it runs, so the roster row exists the
                // moment the first hook fires. A node that refuses leaves
                // the harness a plain terminal rather than unopened.
                let registered = if scope.has_a_roster_row() {
                    match open_interactive(&api_base, &api_token, scope, &id, h) {
                        Ok(i) => Some(i),
                        Err(e) => {
                            tracing::warn!(target: "bisa_desktop", harness = %h, scope = ?scope, id = %id, "the node refused to register a harness session; it opens as a plain terminal: {e}");
                            None
                        }
                    }
                } else {
                    None
                };
                let interactive = registered.filter(|i| i.session.is_some());
                (Some(launch), interactive)
            }
            (None, None, false) => (None, None),
        };
        let sink: Sink = Arc::new(move |event| on_output.send(event).is_ok());
        let session = interactive.as_ref().and_then(|i| i.session.clone());
        // A zero-sized pty makes programs that ask for the window size divide
        // by it. The frontend measures before it mounts, so zero here means the
        // panel had no layout yet, not that the user wants no terminal.
        let spawned = spawn_session(
            registry,
            &dir,
            None,
            launch.as_ref(),
            harness.as_deref(),
            resume,
            interactive.as_ref().map(|i| (i, api_base.as_str())),
            rows.max(1),
            cols.max(1),
            sink,
        );
        let terminal_id = match spawned {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(target: "bisa_desktop", harness = harness.as_deref().unwrap_or("shell"), scope = ?scope, id = %id, "a terminal could not be started: {e}");
                // Registered, never run: the roster row leaves with the error
                // rather than standing as a harness that is starting forever.
                if let Some(reporter) = interactive
                    .as_ref()
                    .and_then(|i| Reporter::from_plan(i, &api_base))
                {
                    reporter.closed();
                }
                return Err(e);
            }
        };
        watcher.ensure_watching(api_base.clone(), api_token.clone());
        Ok(TerminalOpened {
            terminal_id,
            session,
        })
    })
    .await
    .map_err(|e| format!("the terminal thread failed to start: {e}"))?
}

/// A code host sign-in a terminal is opened for (Settings › Git & code hosts
/// › *Authenticate*): which kind, at which host. The program and its argv are
/// this shell's, from [`login_launch`]'s closed table — the webview names a
/// kind and a host, never a command (ide/01).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LoginSpec {
    pub kind: String,
    pub host: String,
}

/// The CLI's own browser login for a kind: `gh auth login --hostname <host>
/// --web --git-protocol https` for GitHub, `glab auth login --hostname
/// <host> --use-keyring --git-protocol https` for GitLab. Bitbucket has no
/// CLI, and a kind this table does not know is a refusal. The host is checked
/// against a hostname's grammar before it is handed to a shell, so a `-` or a
/// `;` cannot arrive as a flag or a second command.
fn login_launch(spec: &LoginSpec) -> Result<Launch, String> {
    let host = spec.host.trim().to_ascii_lowercase();
    let hostname = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !host.starts_with('-')
        && !host.starts_with('.');
    if !hostname {
        return Err(format!("{:?} is not a host name", spec.host));
    }
    let (program, args): (&str, Vec<&str>) = match spec.kind.as_str() {
        "github" => (
            "gh",
            vec![
                "auth",
                "login",
                "--hostname",
                &host,
                "--web",
                "--git-protocol",
                "https",
            ],
        ),
        "gitlab" => (
            "glab",
            vec![
                "auth",
                "login",
                "--hostname",
                &host,
                "--use-keyring",
                "--git-protocol",
                "https",
            ],
        ),
        "bitbucket" => {
            return Err(
                "Bitbucket has no CLI to sign in with — add an API token in Settings instead"
                    .into(),
            )
        }
        other => {
            return Err(format!(
                "{other:?} is not a code host this shell can sign in to"
            ))
        }
    };
    Ok(Launch {
        program: program.to_string(),
        args: args.into_iter().map(str::to_string).collect(),
        resume_args: Vec::new(),
    })
}

#[cfg(test)]
mod login_tests {
    use super::*;

    #[test]
    fn a_sign_in_is_the_clis_own_browser_login_from_a_closed_table() {
        let gh = login_launch(&LoginSpec {
            kind: "github".into(),
            host: "GitHub.com".into(),
        })
        .unwrap();
        assert_eq!(gh.program, "gh");
        assert_eq!(
            gh.args,
            [
                "auth",
                "login",
                "--hostname",
                "github.com",
                "--web",
                "--git-protocol",
                "https"
            ]
        );
        assert!(gh.resume_args.is_empty());
        let glab = login_launch(&LoginSpec {
            kind: "gitlab".into(),
            host: "git.acme.internal".into(),
        })
        .unwrap();
        assert_eq!(glab.program, "glab");
        assert!(glab.args.contains(&"git.acme.internal".to_string()));
        assert!(
            login_launch(&LoginSpec {
                kind: "bitbucket".into(),
                host: "bitbucket.org".into()
            })
            .is_err(),
            "no CLI"
        );
        assert!(
            login_launch(&LoginSpec {
                kind: "gitea".into(),
                host: "x.test".into()
            })
            .is_err(),
            "not a kind this shell knows"
        );
        for bad in ["-flag", "a b", "host;rm", "", "$(x)"] {
            assert!(
                login_launch(&LoginSpec {
                    kind: "github".into(),
                    host: bad.into()
                })
                .is_err(),
                "{bad:?} is not a host name"
            );
        }
        let line = shell_command(&gh, false, &[]);
        assert!(
            line.starts_with("exec 'gh' 'auth' 'login' '--hostname' 'github.com'"),
            "every word is quoted, the CLI's own included: {line}"
        );
    }
}

/// What `terminal_open` answers: the PTY, and the roster session the harness
/// in it reports as — `None` for a plain shell or a harness that cannot report.
#[derive(Clone, Debug, Serialize)]
pub struct TerminalOpened {
    pub terminal_id: String,
    pub session: Option<String>,
}

#[tauri::command]
pub async fn terminal_write(
    registry: State<'_, Arc<TerminalRegistry>>,
    id: String,
    data: String,
) -> Result<(), String> {
    // A child that stopped draining its PTY blocks the write: that wait is
    // the blocking pool's, never the window's main thread.
    let registry = Arc::clone(&registry);
    tauri::async_runtime::spawn_blocking(move || registry.write(&id, &data))
        .await
        .map_err(|e| format!("the terminal write did not finish: {e}"))?
}

/// The person answered the harness's dialog in this tab — the webview saw
/// the roster say *waiting* and the keys that answer a dialog typed. The
/// node is told through the session's own door; a post is a TCP call, so
/// it runs off the main thread like a write.
#[tauri::command]
pub async fn terminal_answered(
    registry: State<'_, Arc<TerminalRegistry>>,
    id: String,
) -> Result<(), String> {
    let registry = Arc::clone(&registry);
    tauri::async_runtime::spawn_blocking(move || registry.answered(&id))
        .await
        .map_err(|e| format!("the answer did not reach the node: {e}"))
}

#[tauri::command]
pub fn terminal_resize(
    registry: State<'_, Arc<TerminalRegistry>>,
    id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    registry.resize(&id, rows.max(1), cols.max(1))
}

/// The tab is gone. Off the main thread: ending a process and telling the
/// roster are both things that wait on somebody else.
#[tauri::command]
pub async fn terminal_close(
    registry: State<'_, Arc<TerminalRegistry>>,
    id: String,
) -> Result<(), String> {
    let registry = Arc::clone(registry.inner());
    tauri::async_runtime::spawn_blocking(move || registry.terminate(&id))
        .await
        .map_err(|e| format!("the close did not finish: {e}"))
}

/// Where a shell stands now — read at a click on a relative path it printed
/// (ide/17), so the path is read from where the tool that printed it ran. The
/// process table is a syscall away: off the main thread, like a write.
#[tauri::command]
pub async fn terminal_cwd(
    registry: State<'_, Arc<TerminalRegistry>>,
    id: String,
) -> Result<String, String> {
    let registry = Arc::clone(registry.inner());
    tauri::async_runtime::spawn_blocking(move || registry.cwd(&id))
        .await
        .map_err(|e| format!("the directory read did not finish: {e}"))?
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// What runs in a shell
// ---------------------------------------------------------------------------

/// How often the process table is read while any shell is live — the port
/// scanner's cadence, halved: a harness typed into a shell shows as one
/// within two seconds, and a read costs tens of milliseconds.
const WATCH_EVERY: Duration = Duration::from_secs(2);
/// How long the catalog's programs are trusted before `GET /harnesses` is
/// asked again — a custom descriptor added while a shell is open.
const PROGRAMS_TTL: Duration = Duration::from_secs(60);

/// The interpreters a harness may run under: an npm-installed `claude` is a
/// `node` process whose script is the program, so the script's name counts.
const INTERPRETERS: &[&str] = &["node", "bun", "deno", "python", "python3", "ruby", "perl"];

/// One process as the table shows it, cut down to what the match reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessFacts {
    pub exe: Option<String>,
    pub argv: Vec<String>,
}

/// The last path segment, or the whole word when there is none.
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Which harness a process is, if it is one the catalog names: its
/// executable's name, its `argv[0]`'s, or — under an interpreter — its
/// script's. `programs` maps a program's name to the harness id.
pub(crate) fn harness_of(
    facts: &ProcessFacts,
    programs: &HashMap<String, String>,
) -> Option<String> {
    let exe = facts.exe.as_deref().map(basename);
    let argv0 = facts.argv.first().map(|a| basename(a));
    for name in [exe, argv0].into_iter().flatten() {
        if let Some(id) = programs.get(name) {
            return Some(id.clone());
        }
    }
    let under_interpreter = [exe, argv0]
        .into_iter()
        .flatten()
        .any(|name| INTERPRETERS.contains(&name));
    if under_interpreter {
        if let Some(script) = facts.argv.get(1).map(|a| basename(a)) {
            if let Some(id) = programs.get(script) {
                return Some(id.clone());
            }
        }
    }
    None
}

/// The harness running under a shell's pid — the shell itself included, since
/// a launched harness is `exec`'d over it — breadth-first, so the one nearest
/// the shell wins when a harness spawns another.
pub(crate) fn harness_under(
    root: u32,
    children: &HashMap<u32, Vec<u32>>,
    facts: &HashMap<u32, ProcessFacts>,
    programs: &HashMap<String, String>,
) -> Option<String> {
    let mut queue = std::collections::VecDeque::from([root]);
    let mut seen = std::collections::HashSet::new();
    while let Some(pid) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(f) = facts.get(&pid) {
            if let Some(id) = harness_of(f, programs) {
                return Some(id);
            }
        }
        if let Some(kids) = children.get(&pid) {
            queue.extend(kids.iter().copied());
        }
    }
    None
}

/// The catalog's programs by name, from `GET /harnesses` — every row with
/// an interactive form, installed or not (a harness typed by hand is
/// installed by definition).
fn harness_programs(api_base: &str, token: &str) -> Result<HashMap<String, String>, String> {
    let (host, port) = authority(api_base)?;
    let (status, body) = parse_http_response(&get(&host, port, token, "/harnesses")?)?;
    if status != 200 {
        return Err(node_error(status, &body));
    }
    let list: HarnessList = serde_json::from_str(&body)
        .map_err(|e| format!("the node's harness list did not parse: {e}"))?;
    Ok(list
        .harnesses
        .into_iter()
        .filter_map(|h| h.launch.map(|l| (basename(&l.program).to_string(), h.id)))
        .collect())
}

/// The process table once: every pid's parent, executable and argv.
fn process_table() -> (HashMap<u32, Vec<u32>>, HashMap<u32, ProcessFacts>) {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet),
        ),
    );
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut facts = HashMap::new();
    for (pid, p) in sys.processes() {
        let pid = pid.as_u32();
        if let Some(parent) = p.parent() {
            children.entry(parent.as_u32()).or_default().push(pid);
        }
        facts.insert(
            pid,
            ProcessFacts {
                exe: p
                    .exe()
                    .map(|e| e.display().to_string())
                    .or_else(|| Some(p.name().to_string_lossy().into_owned())),
                argv: p
                    .cmd()
                    .iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
            },
        );
    }
    (children, facts)
}

/// The watcher: while any shell is live, read the process table every
/// `WATCH_EVERY` and tell each tab which harness runs under it. Stops
/// itself when nothing is live; the next open starts it again.
fn watch_processes(registry: Arc<TerminalRegistry>, api_base: &str, token: &str) {
    let mut programs: HashMap<String, String> = HashMap::new();
    let mut fetched: Option<Instant> = None;
    loop {
        let live = registry.pids();
        if live.is_empty() || registry.closed.load(Ordering::SeqCst) {
            registry.watching.store(false, Ordering::SeqCst);
            return;
        }
        if fetched.is_none_or(|at| at.elapsed() > PROGRAMS_TTL) {
            match harness_programs(api_base, token) {
                Ok(p) => {
                    programs = p;
                    fetched = Some(Instant::now());
                }
                Err(e) => {
                    tracing::debug!(target: "bisa_desktop", "the harness list did not answer: {e}")
                }
            }
        }
        if !programs.is_empty() {
            let (children, facts) = process_table();
            for (id, pid) in live {
                registry.note_running(&id, harness_under(pid, &children, &facts, &programs));
            }
        }
        std::thread::sleep(WATCH_EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // --- what runs in a shell ---------------------------------------------

    fn programs() -> HashMap<String, String> {
        HashMap::from([
            ("claude".to_string(), "claude-code".to_string()),
            ("codex".to_string(), "codex".to_string()),
            ("goose".to_string(), "preset:goose".to_string()),
        ])
    }

    fn facts(exe: &str, argv: &[&str]) -> ProcessFacts {
        ProcessFacts {
            exe: Some(exe.to_string()),
            argv: argv.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn harness_of_reads_the_executable_the_argv_or_the_script_under_an_interpreter() {
        let p = programs();
        assert_eq!(
            harness_of(&facts("/opt/homebrew/bin/claude", &["claude"]), &p).as_deref(),
            Some("claude-code")
        );
        assert_eq!(
            harness_of(
                &facts("/usr/local/bin/codex", &["codex", "--full-auto"]),
                &p
            )
            .as_deref(),
            Some("codex")
        );
        // An npm-installed harness: a `node` whose script is the program.
        assert_eq!(
            harness_of(
                &facts(
                    "/usr/local/bin/node",
                    &[
                        "node",
                        "/usr/local/lib/node_modules/@anthropic-ai/claude-code/bin/claude"
                    ]
                ),
                &p
            )
            .as_deref(),
            Some("claude-code")
        );
        // The exe unknown, argv[0] the program: still the harness.
        assert_eq!(
            harness_of(
                &ProcessFacts {
                    exe: None,
                    argv: vec!["goose".into()]
                },
                &p
            )
            .as_deref(),
            Some("preset:goose")
        );
        // Anything else is not a harness — an editor, a shell, node running something else.
        assert_eq!(
            harness_of(&facts("/usr/bin/vim", &["vim", "claude"]), &p),
            None,
            "a script argument is read only under an interpreter"
        );
        assert_eq!(harness_of(&facts("/bin/zsh", &["-zsh"]), &p), None);
        assert_eq!(
            harness_of(&facts("/usr/local/bin/node", &["node", "server.js"]), &p),
            None
        );
        assert_eq!(harness_of(&ProcessFacts::default(), &p), None);
    }

    #[test]
    fn harness_under_finds_the_nearest_one_below_the_shell_or_the_shell_itself_when_exec_d() {
        let p = programs();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut table: HashMap<u32, ProcessFacts> = HashMap::new();
        // 10 zsh → 11 claude → 12 node (a tool claude spawned) → 13 codex (a harness under a harness)
        children.insert(10, vec![11]);
        children.insert(11, vec![12]);
        children.insert(12, vec![13]);
        table.insert(10, facts("/bin/zsh", &["-zsh"]));
        table.insert(11, facts("/opt/homebrew/bin/claude", &["claude"]));
        table.insert(12, facts("/usr/local/bin/node", &["node", "tool.js"]));
        table.insert(13, facts("/usr/local/bin/codex", &["codex"]));
        assert_eq!(
            harness_under(10, &children, &table, &p).as_deref(),
            Some("claude-code"),
            "the nearest to the shell"
        );
        assert_eq!(
            harness_under(12, &children, &table, &p).as_deref(),
            Some("codex")
        );
        // A plain shell running an editor is a shell.
        let mut quiet: HashMap<u32, ProcessFacts> = HashMap::new();
        quiet.insert(10, facts("/bin/zsh", &["-zsh"]));
        quiet.insert(11, facts("/usr/bin/vim", &["vim"]));
        assert_eq!(
            harness_under(10, &HashMap::from([(10, vec![11])]), &quiet, &p),
            None
        );
        // A launched harness tab: the shell exec'd the program, so the root is the harness.
        let mut execd: HashMap<u32, ProcessFacts> = HashMap::new();
        execd.insert(
            10,
            facts("/opt/homebrew/bin/claude", &["claude", "--continue"]),
        );
        assert_eq!(
            harness_under(10, &HashMap::new(), &execd, &p).as_deref(),
            Some("claude-code")
        );
        // A pid the table does not know is nothing, and a cycle ends.
        assert_eq!(harness_under(99, &children, &table, &p), None);
        assert_eq!(
            harness_under(10, &HashMap::from([(10, vec![10])]), &quiet, &p),
            None
        );
    }

    #[test]
    fn note_running_tells_the_tab_once_per_change_and_never_a_dead_one() {
        let registry = Arc::new(TerminalRegistry::default());
        let seen = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
        let sink_seen = Arc::clone(&seen);
        let sink: Sink = Arc::new(move |event| {
            if let TerminalEvent::Process { harness } = event {
                sink_seen.lock().unwrap().push(harness);
            }
            true
        });
        registry.insert(
            "term-1".into(),
            Session {
                phase: Phase::Live,
                master: None,
                writer: None,
                killer: None,
                pid: Some(4242),
                dir: PathBuf::from("/tmp"),
                reporter: None,
                sink: Some(sink),
                running: None,
            },
        );
        registry.note_running("term-1", None);
        registry.note_running("term-1", Some("claude-code".into()));
        registry.note_running("term-1", Some("claude-code".into()));
        registry.note_running("term-1", None);
        registry.note_running("nowhere", Some("codex".into()));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![Some("claude-code".to_string()), None],
            "the same answer twice says nothing"
        );
        registry.finished("term-1", Some(0), None, true);
        registry.note_running("term-1", Some("codex".into()));
        assert_eq!(
            seen.lock().unwrap().len(),
            2,
            "an exited tab is told nothing"
        );
    }

    #[test]
    fn a_scope_the_node_does_not_have_is_refused_rather_than_defaulted() {
        let err = Scope::parse("../../etc").unwrap_err();
        assert!(err.contains("unknown terminal scope"), "{err}");
        assert!(err.contains("goal, workstream, work_item"), "{err}");
        assert_eq!(Scope::parse("workstream").unwrap(), Scope::Workstream);
        // The wire spelling has to match `bisa_store::FileScope`, or the
        // placement lookup 400s on a scope this side thinks is fine.
        assert_eq!(Scope::parse("work_item").unwrap(), Scope::WorkItem);
        assert!(Scope::parse("workitem").is_err());
    }

    /// **A harness argument is data, not syntax.**
    ///
    /// `args` comes from a tier-3 custom descriptor — a JSON file a person
    /// wrote — and ends up inside a string a shell parses. Everything a shell
    /// would otherwise act on has to survive as one word.
    #[test]
    fn a_harness_argument_cannot_become_a_second_command() {
        assert_eq!(shell_word("claude"), "'claude'");
        assert_eq!(shell_word(""), "''");
        assert_eq!(shell_word("--model=opus"), "'--model=opus'");
        // The only character single quotes cannot hold.
        assert_eq!(shell_word("it's"), r"'it'\''s'");
        // Everything a shell would otherwise act on, inert.
        for hostile in [
            "; rm -rf /",
            "$(whoami)",
            "`whoami`",
            "a\nb",
            "&& echo pwned",
            "$HOME",
            "*",
        ] {
            let quoted = shell_word(hostile);
            assert!(
                quoted.starts_with('\'') && quoted.ends_with('\''),
                "{quoted}"
            );
            // No unescaped quote can end the run early, which is the only way
            // any of the above could reach the parser as syntax.
            let inner = &quoted[1..quoted.len() - 1];
            assert!(!inner.contains('\''), "{quoted} can be closed early");
        }

        let cmd = shell_command(
            &Launch {
                program: "my agent".into(),
                args: vec!["--flag".into(), "; rm -rf /".into()],
                resume_args: vec![],
            },
            false,
            &[],
        );
        assert_eq!(cmd, r"exec 'my agent' '--flag' '; rm -rf /'");
    }

    /// Three refusals, because they are three different things to fix.
    #[test]
    fn a_harness_the_node_cannot_run_is_refused_with_the_reason() {
        // Assembled by hand rather than through a live node: the point under
        // test is the decision, and the three shapes it decides on.
        let list: HarnessList = serde_json::from_str(
            r#"{"harnesses":[
                 {"id":"claude-code","label":"Claude Code","installed":true,
                  "launch":{"program":"claude","args":[]}},
                 {"id":"acp:goose","label":"Goose (ACP)","installed":true,"launch":null},
                 {"id":"codex","label":"Codex","installed":false,
                  "launch":{"program":"codex"}}
               ]}"#,
        )
        .expect("the shape the node serves");

        let row = |id: &str| list.harnesses.iter().find(|h| h.id == id).unwrap();
        assert_eq!(
            row("claude-code").launch.as_ref().unwrap().program,
            "claude"
        );
        // `args` is optional on the wire — a bare program is the common case.
        assert!(row("codex").launch.as_ref().unwrap().args.is_empty());
        assert!(row("acp:goose").launch.is_none());
        assert!(!row("codex").installed);
    }

    #[test]
    fn an_api_base_the_terminal_cannot_reach_over_loopback_http_is_refused() {
        assert_eq!(
            authority("http://127.0.0.1:4477").unwrap(),
            ("127.0.0.1".to_string(), 4477)
        );
        // A trailing path is the shape `NodeState` hands back after trimming,
        // and a bare host is legal even though the node never emits one.
        assert_eq!(
            authority("http://localhost:9/health").unwrap(),
            ("localhost".to_string(), 9)
        );
        assert_eq!(authority("http://node").unwrap(), ("node".to_string(), 80));
        assert!(authority("https://example.com").is_err());
        assert!(authority("unix:/tmp/bisa.sock").is_err());
        assert!(authority("http://:4477").is_err());
        assert!(authority("http://host:not-a-port").is_err());
    }

    #[test]
    fn an_id_cannot_smuggle_a_second_request_into_the_placement_get() {
        assert_eq!(encode_segment("01J8ZQ-abc_9.x~"), "01J8ZQ-abc_9.x~");
        assert_eq!(
            encode_segment("a\r\nGET /evil HTTP/1.1"),
            "a%0D%0AGET%20%2Fevil%20HTTP%2F1.1"
        );
        assert_eq!(encode_segment("../.."), "..%2F..");
        // Non-ASCII goes out as its UTF-8 bytes, one escape each.
        assert_eq!(encode_segment("é"), "%C3%A9");
    }

    #[test]
    fn the_placement_answer_is_read_off_a_raw_socket_status_line_first() {
        let ok = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 30\r\n\r\n{\"path\":\"/tmp/x\",\"exists\":true}";
        let (status, body) = parse_http_response(ok).unwrap();
        assert_eq!(status, 200);
        let placement: Placement = serde_json::from_str(&body).unwrap();
        assert_eq!(placement.path, "/tmp/x");
        assert!(placement.exists);
    }

    /// The three places a terminal can be rooted in, and nothing else: a scope
    /// the node does not resolve a directory for is refused at the IPC boundary.
    #[test]
    fn a_harness_is_a_roster_row_wherever_the_node_places_a_terminal_and_nowhere_else() {
        for scope in Scope::ALL {
            assert_eq!(
                scope.has_a_roster_row(),
                scope != Scope::Machine,
                "{scope:?}"
            );
        }
    }

    #[test]
    fn a_scope_the_host_does_not_know_is_refused() {
        assert_eq!(Scope::parse("workstream").unwrap(), Scope::Workstream);
        let err = Scope::parse("tool").unwrap_err();
        assert!(err.contains("goal") && err.contains("work_item"), "{err}");
    }

    #[test]
    fn a_refusal_from_the_node_keeps_the_message_the_node_wrote() {
        let raw = "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\n\r\n{\"error\":\"no such goal 01J8\"}";
        let (status, body) = parse_http_response(raw).unwrap();
        assert_eq!(status, 404);
        assert_eq!(node_error(status, &body), "no such goal 01J8");
        // A body that is not the node's error shape still says something.
        assert_eq!(node_error(500, "<html>"), "the node answered 500");
    }

    #[test]
    fn a_response_this_minimal_client_cannot_read_is_refused_not_guessed_at() {
        assert!(parse_http_response("HTTP/1.1 200 OK\r\ncontent-length: 2").is_err());
        assert!(parse_http_response("garbage\r\n\r\n{}").is_err());
        let chunked = "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
        assert!(parse_http_response(chunked).is_err());
    }

    #[test]
    fn a_character_split_across_two_reads_is_delivered_once_and_whole() {
        let mut chunks = Utf8Chunks::default();
        let smiley = "🙂".as_bytes();
        assert_eq!(chunks.decode(&smiley[..2]), "");
        assert_eq!(chunks.decode(&smiley[2..]), "🙂");

        // The tail carries over while the rest of the read is delivered.
        let mut chunks = Utf8Chunks::default();
        let mixed = "ok é".as_bytes();
        assert_eq!(chunks.decode(&mixed[..4]), "ok ");
        assert_eq!(chunks.decode(&mixed[4..]), "é");
    }

    #[test]
    fn genuinely_invalid_bytes_are_marked_rather_than_dropped() {
        let mut chunks = Utf8Chunks::default();
        assert_eq!(chunks.decode(&[b'a', 0xFF, b'b']), "a\u{FFFD}b");
    }

    #[test]
    fn terminal_ids_are_unique_within_a_run() {
        let registry = TerminalRegistry::new();
        let ids: Vec<String> = (0..100).map(|_| registry.next_id()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert_eq!(ids[0], "term-1");
    }

    #[test]
    fn writing_to_a_terminal_that_is_not_open_is_refused() {
        let registry = TerminalRegistry::new();
        let err = registry.write("term-99", "pwd\n").unwrap_err();
        assert!(err.contains("no terminal"), "{err}");
        assert!(registry.resize("term-99", 24, 80).is_err());
    }

    #[test]
    fn closing_a_terminal_that_has_already_gone_is_not_an_error() {
        let registry = Arc::new(TerminalRegistry::new());
        registry.terminate("term-99");
        assert_eq!(registry.len(), 0);
    }

    /// Ending a tab is asynchronous by design — the pump alone waits on the
    /// child — so a test asks the registry rather than assuming.
    fn wait_gone(registry: &Arc<TerminalRegistry>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while registry.len() > 0 {
            assert!(
                Instant::now() < deadline,
                "the registry still holds {} shells",
                registry.len()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn the_default_program_is_the_login_shell_and_not_a_hardcoded_one() {
        let cmd = CommandBuilder::new_default_prog();
        assert!(cmd.is_default_prog());
        assert!(!cmd.get_shell().is_empty());
    }

    fn launch_of(program: &str, args: &[&str], resume_args: &[&str]) -> Launch {
        Launch {
            program: program.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            resume_args: resume_args.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    fn argv(cmd: &CommandBuilder) -> Vec<String> {
        cmd.get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_harness_on_the_login_shell_builds_a_command_instead_of_panicking() {
        // The combination that shipped and the one nothing constructed: the
        // production `program: None` *with* a harness to run. `CommandBuilder::
        // arg` panics on a default-prog builder, so every harness terminal died
        // here while the plain shell — which never appends an arg — was fine.
        let launch = launch_of("claude", &[], &[]);
        let cmd = build_command(None, Some(&launch), false, &[]);

        assert!(
            !cmd.is_default_prog(),
            "the shell has to be named before anything can be appended to it"
        );
        let argv = argv(&cmd);
        assert_eq!(argv.len(), 4, "{argv:?}");
        assert!(!argv[0].is_empty(), "argv[0] is the resolved login shell");
        assert_eq!(
            &argv[1], "-l",
            "a login shell, since argv[0] carries no '-'"
        );
        assert_eq!(&argv[2], "-c");
        assert_eq!(&argv[3], "exec 'claude'");
    }

    #[test]
    fn a_plain_shell_is_still_the_default_program() {
        // The other half of the same rule: with nothing to run *in* the shell
        // there is nothing to append, so the builder stays the default one and
        // keeps its `-zsh` argv[0].
        let cmd = build_command(None, None, false, &[]);
        assert!(cmd.is_default_prog());
        assert!(argv(&cmd).is_empty());
    }

    #[test]
    fn resuming_appends_the_harness_own_flag_and_nothing_else() {
        let launch = launch_of("claude", &[], &["--continue"]);

        let fresh = argv(&build_command(None, Some(&launch), false, &[]));
        assert_eq!(fresh[3], "exec 'claude'");

        let resumed = argv(&build_command(None, Some(&launch), true, &[]));
        assert_eq!(resumed[3], "exec 'claude' '--continue'");

        // Ordinary args come first and are present either way: they say which
        // program this is, and the resume flag only says which session.
        let with_args = launch_of("omp", &["--wrapper"], &["--resume"]);
        assert_eq!(
            argv(&build_command(None, Some(&with_args), true, &[]))[3],
            "exec 'omp' '--wrapper' '--resume'"
        );
    }

    #[test]
    fn asking_a_harness_with_no_resume_form_to_resume_opens_it_fresh() {
        // Nothing has to check whether resuming is possible before asking for
        // it: an empty `resume_args` appends nothing, so the two commands are
        // the same string rather than one of them being an error.
        let launch = launch_of("goose", &[], &[]);
        assert_eq!(
            argv(&build_command(None, Some(&launch), true, &[])),
            argv(&build_command(None, Some(&launch), false, &[])),
        );
    }

    #[test]
    fn a_resume_flag_is_quoted_like_every_other_word() {
        // `resume_args` reaches this string from a tier-3 descriptor somebody
        // wrote, by the same route as `args`, and lands in a command a shell is
        // about to parse. It is data, not syntax.
        let launch = launch_of("tool", &[], &["; rm -rf ~", "$(whoami)"]);
        let line = argv(&build_command(None, Some(&launch), true, &[])).remove(3);
        assert_eq!(line, "exec 'tool' '; rm -rf ~' '$(whoami)'");
        assert!(!line.contains("$( "), "no unquoted substitution: {line}");
    }

    #[test]
    fn a_reporting_plan_s_words_come_last_and_are_quoted_too() {
        // The plan's arguments name files the node wrote and flags the harness
        // reads after its own; they land after the resume flag, as data.
        let launch = launch_of("claude", &[], &["--continue"]);
        let extra = vec![
            "--settings".to_string(),
            "/ws/run/interactive/01S/it's.json".to_string(),
        ];
        let line = argv(&build_command(None, Some(&launch), true, &extra)).remove(3);
        assert_eq!(
            line,
            r"exec 'claude' '--continue' '--settings' '/ws/run/interactive/01S/it'\''s.json'"
        );
    }

    #[test]
    fn the_osc9_scanner_finds_a_notification_across_reads_and_ignores_plain_output() {
        let mut scanner = Osc9Scanner::default();
        assert!(scanner.feed("plain output\r\n").is_empty());
        assert_eq!(
            scanner.feed("\x1b]9;Approval requested\x07more"),
            vec!["Approval requested"]
        );
        // Split across two reads, terminated by ST rather than BEL.
        assert!(scanner.feed("x\x1b]9;agent-turn").is_empty());
        assert_eq!(
            scanner.feed(" complete\x1b\\tail"),
            vec!["agent-turn complete"]
        );
        // Two in one chunk.
        assert_eq!(scanner.feed("\x1b]9;a\x07\x1b]9;b\x07"), vec!["a", "b"]);
    }

    #[test]
    fn a_shell_will_not_open_in_a_path_that_is_not_a_directory() {
        let registry = Arc::new(TerminalRegistry::new());
        let missing = std::path::PathBuf::from("/definitely/not/here/at/all");
        let err = spawn_session(
            Arc::clone(&registry),
            &missing,
            Some("/bin/sh"),
            None,
            None,
            false,
            None,
            24,
            80,
            Arc::new(|_| true),
        )
        .unwrap_err();
        assert!(err.contains("is not a directory"), "{err}");
        assert_eq!(registry.len(), 0);
    }

    /// The load-bearing test: the whole point of resolving a placement is that
    /// the shell lands in that directory, and nothing short of asking a real
    /// shell where it is proves it.
    ///
    /// `/bin/sh` rather than the login shell so the answer does not depend on
    /// what the developer's profile prints, and `pwd` because it is the only
    /// question worth asking here.
    #[cfg(unix)]
    #[test]
    fn a_shell_opens_in_the_directory_it_was_given_and_pwd_says_so() {
        let dir = tempfile::tempdir().unwrap();
        // macOS hands out `/var/folders/...`, which is a symlink to
        // `/private/var/folders/...`; the shell reports the resolved path
        // because that is what `getcwd` returns.
        let want = std::fs::canonicalize(dir.path()).unwrap();
        let want_str = want.display().to_string();
        // The falsification: if `cwd` silently did nothing, the shell would
        // inherit this process's directory and the assertion below would pass
        // on a substring that means nothing.
        let inherited = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        assert!(want.starts_with("/") && !want.starts_with(&inherited));

        let seen = Arc::new(Mutex::new(String::new()));
        let sink_seen = Arc::clone(&seen);
        let registry = Arc::new(TerminalRegistry::new());
        let id = spawn_session(
            Arc::clone(&registry),
            &want,
            Some("/bin/sh"),
            None,
            None,
            false,
            None,
            24,
            80,
            Arc::new(move |event| {
                if let TerminalEvent::Output { data } = event {
                    sink_seen.lock().unwrap().push_str(&data);
                }
                true
            }),
        )
        .unwrap();
        assert_eq!(registry.len(), 1);

        registry.write(&id, "pwd\n").unwrap();

        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let got = seen.lock().unwrap().clone();
            if got.contains(&want_str) {
                assert!(
                    !got.contains(&inherited.display().to_string()),
                    "the shell also reported the directory it inherited: {got:?}"
                );
                break;
            }
            if Instant::now() >= deadline {
                registry.terminate(&id);
                panic!("the shell never printed {want_str}; it said {got:?}");
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        registry.terminate(&id);
        wait_gone(&registry);
    }

    /// Where a shell stands is the process's own word, read at the moment of
    /// asking: it follows a `cd`, and once the process is gone it is where the
    /// shell was started, so a dead tab's scrollback still reads its paths.
    #[cfg(unix)]
    #[test]
    fn where_a_shell_stands_follows_its_cd_and_outlives_its_process() {
        let dir = tempfile::tempdir().unwrap();
        let want = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(want.join("sub")).unwrap();
        let exited = Arc::new(AtomicBool::new(false));
        let sink_exited = Arc::clone(&exited);
        let registry = Arc::new(TerminalRegistry::new());
        let id = spawn_session(
            Arc::clone(&registry),
            &want,
            Some("/bin/sh"),
            None,
            None,
            false,
            None,
            24,
            80,
            Arc::new(move |event| {
                if matches!(event, TerminalEvent::Exit { .. }) {
                    sink_exited.store(true, Ordering::SeqCst);
                }
                true
            }),
        )
        .unwrap();
        let said = |registry: &TerminalRegistry| PathBuf::from(registry.cwd(&id).unwrap());
        assert_eq!(
            said(&registry),
            want,
            "a fresh shell stands where it was started"
        );

        registry.write(&id, "cd sub\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while said(&registry) != want.join("sub") {
            if Instant::now() >= deadline {
                registry.terminate(&id);
                panic!(
                    "the shell never stood in sub; it said {:?}",
                    said(&registry)
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        // The process ends; the tab, and its answer, stay.
        registry.write(&id, "exit\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !exited.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "the shell never exited");
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(registry.len(), 1, "the tab still owns the session");
        assert_eq!(
            said(&registry),
            want,
            "a shell whose process is gone stands where it was started"
        );

        registry.terminate(&id);
        wait_gone(&registry);
    }

    #[test]
    fn where_a_process_stands_is_the_tables_word_else_where_it_started() {
        let here = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        let reported = cwd_of(std::process::id()).expect("this process has a directory");
        assert_eq!(std::fs::canonicalize(reported).unwrap(), here);
        assert_eq!(
            cwd_of(u32::MAX - 1),
            None,
            "a pid nobody holds says nothing"
        );
        let started = Path::new("/started/here");
        assert_eq!(place_of(None, started), started);
        assert_eq!(
            place_of(Some(PathBuf::from("/moved/to")), started),
            Path::new("/moved/to")
        );
        let registry = TerminalRegistry::new();
        let err = registry.cwd("term-99").unwrap_err();
        assert!(err.contains("no terminal"), "{err}");
    }

    /// The load-bearing test for running a harness: the program the node named
    /// is what starts, **in the resolved directory**, with its arguments intact
    /// as separate words.
    ///
    /// A stub script rather than a real harness, for the reason the `pwd` test
    /// above uses `/bin/sh`: this is about the plumbing, not about whether a
    /// developer has Claude Code installed. The stub prints its working
    /// directory and one line per argument, which is the only way to prove a
    /// word survived — `$*` would join them back together and hide a split.
    ///
    /// The arguments are deliberately hostile. `it'"'"'s; fine` contains the one
    /// character single quotes cannot hold *and* a command separator, so if
    /// `shell_word` were wrong this run would either fail outright or execute
    /// `fine` as a second command instead of printing one argument.
    #[cfg(unix)]
    #[test]
    fn a_harness_starts_in_the_resolved_directory_with_its_arguments_intact() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let want = std::fs::canonicalize(dir.path()).unwrap();
        let want_str = want.display().to_string();
        let inherited = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        assert!(want.starts_with("/") && !want.starts_with(&inherited));

        let stub = want.join("stub-harness");
        std::fs::write(
            &stub,
            "#!/bin/sh\npwd\nfor a in \"$@\"; do printf 'ARG[%s]\\n' \"$a\"; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let launch = Launch {
            program: stub.display().to_string(),
            args: vec!["--note".into(), "it's; fine".into()],
            resume_args: vec!["--resume".into()],
        };

        let seen = Arc::new(Mutex::new(String::new()));
        let sink_seen = Arc::clone(&seen);
        let registry = Arc::new(TerminalRegistry::new());
        let id = spawn_session(
            Arc::clone(&registry),
            &want,
            Some("/bin/sh"),
            Some(&launch),
            Some("stub"),
            true,
            None,
            24,
            80,
            Arc::new(move |event| {
                if let TerminalEvent::Output { data } = event {
                    sink_seen.lock().unwrap().push_str(&data);
                }
                true
            }),
        )
        .unwrap();

        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let got = seen.lock().unwrap().clone();
            if got.contains("ARG[it's; fine]") {
                assert!(
                    got.contains(&want_str),
                    "the harness did not start in {want_str}: {got:?}"
                );
                assert!(got.contains("ARG[--note]"), "{got:?}");
                // Opened with `resume`, so the harness's own flag reached the
                // process — after its ordinary args, which is the order that
                // makes it a modifier of this program rather than a new one.
                assert!(
                    got.contains("ARG[--resume]"),
                    "the resume flag never reached the harness: {got:?}"
                );
                // If the semicolon had reached the shell as syntax, the second
                // word would have been split and this line would not exist as
                // one argument.
                assert!(
                    !got.contains("ARG[it's]"),
                    "the argument was split at the semicolon: {got:?}"
                );
                break;
            }
            if Instant::now() >= deadline {
                registry.terminate(&id);
                panic!("the harness never printed its arguments; it said {got:?}");
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        registry.terminate(&id);
        wait_gone(&registry);
    }

    /// The leak this whole registry exists to prevent: a panel that unmounts
    /// without closing leaves one shell process per visit.
    #[cfg(unix)]
    #[test]
    fn shutting_down_takes_every_shell_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Arc::new(TerminalRegistry::new());
        for _ in 0..3 {
            spawn_session(
                Arc::clone(&registry),
                dir.path(),
                Some("/bin/sh"),
                None,
                None,
                false,
                None,
                24,
                80,
                Arc::new(|_| true),
            )
            .unwrap();
        }
        assert_eq!(registry.len(), 3);
        registry.shutdown_all();
        wait_gone(&registry);
        // Late arrivals are ended at once rather than outliving the window.
        assert!(!registry.insert(
            "term-late".into(),
            Session {
                phase: Phase::Live,
                master: None,
                writer: None,
                killer: None,
                pid: None,
                dir: PathBuf::from("/tmp"),
                sink: None,
                running: None,
                reporter: None,
            }
        ));
        assert_eq!(registry.len(), 0);
    }

    /// A shell that ended by itself keeps its entry, so the tab that still
    /// shows its last words can close it — and say so to the roster — later.
    #[cfg(unix)]
    #[test]
    fn a_shell_that_exits_by_itself_stays_registered_until_its_tab_closes() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Arc::new(TerminalRegistry::new());
        let exit = Arc::new(Mutex::new(None));
        let seen = Arc::clone(&exit);
        let id = spawn_session(
            Arc::clone(&registry),
            dir.path(),
            Some("/bin/sh"),
            None,
            None,
            false,
            None,
            24,
            80,
            Arc::new(move |event| {
                if let TerminalEvent::Exit { code } = event {
                    *seen.lock().unwrap() = Some(code);
                }
                true
            }),
        )
        .unwrap();
        registry.write(&id, "exit 3\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while exit.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the shell never exited");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(*exit.lock().unwrap(), Some(Some(3)));
        assert_eq!(registry.len(), 1, "the tab still owns its entry");
        assert!(registry
            .write(&id, "echo\n")
            .unwrap_err()
            .contains("has exited"));
        assert!(
            registry.pids().is_empty(),
            "an exited shell is not a port root"
        );
        registry.terminate(&id);
        assert_eq!(registry.len(), 0);
    }
}

// ---------------------------------------------------------------------------
// Scrollback checkpoints (ide/06, ADR-0030)
// ---------------------------------------------------------------------------
//
// The emulator serialises its buffer in the webview; the shell keeps it at
// `run/terminals/<key>.scrollback` inside the workspace, which it learns from
// the node's own `/workspace` answer (`data_dir`) rather than guessing. The
// key is the tab's `t<n>`; anything else is refused, so the webview cannot
// turn this into a general file write. Capped, because a checkpoint larger
// than a few megabytes is a runaway program, not scrollback.

/// The most bytes one checkpoint may hold.
const SCROLLBACK_CAP: usize = 4 * 1024 * 1024;

static SCROLLBACK_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn scrollback_dir(api_base: &str, token: &str) -> Result<PathBuf, String> {
    if let Some(dir) = SCROLLBACK_DIR.get() {
        return Ok(dir.clone());
    }
    let (host, port) = authority(api_base)?;
    let (status, body) = parse_http_response(&get(&host, port, token, "/workspace")?)?;
    if status != 200 {
        return Err(node_error(status, &body));
    }
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("the node's workspace answer did not parse: {e}"))?;
    let data_dir = v["data_dir"]
        .as_str()
        .ok_or_else(|| "the node's workspace answer has no data_dir".to_string())?;
    let dir = PathBuf::from(data_dir).join("run").join("terminals");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    Ok(SCROLLBACK_DIR.get_or_init(|| dir).clone())
}

fn scrollback_path(api_base: &str, token: &str, key: &str) -> Result<PathBuf, String> {
    let ok = key.len() > 1
        && key.len() <= 24
        && key.starts_with('t')
        && key[1..].bytes().all(|b| b.is_ascii_digit());
    if !ok {
        return Err(format!("{key:?} is not a terminal key"));
    }
    Ok(scrollback_dir(api_base, token)?.join(format!("{key}.scrollback")))
}

#[tauri::command]
pub fn terminal_scrollback_read(
    node: State<'_, NodeState>,
    key: String,
) -> Result<Option<String>, String> {
    let path = scrollback_path(&node.api_base()?, &node.api_token(), &key)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("could not read {}: {e}", path.display())),
    }
}

#[tauri::command]
pub fn terminal_scrollback_write(
    node: State<'_, NodeState>,
    key: String,
    data: String,
) -> Result<(), String> {
    let path = scrollback_path(&node.api_base()?, &node.api_token(), &key)?;
    // Keep the tail: what a person scrolls back to is the recent part.
    let bytes = data.as_bytes();
    let kept = if bytes.len() > SCROLLBACK_CAP {
        let mut start = bytes.len() - SCROLLBACK_CAP;
        while start < bytes.len() && !data.is_char_boundary(start) {
            start += 1;
        }
        &bytes[start..]
    } else {
        bytes
    };
    // Write beside, then rename: a crash mid-write leaves the old checkpoint,
    // never half of a new one.
    let tmp = path.with_extension("scrollback.tmp");
    std::fs::write(&tmp, kept).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("could not place {}: {e}", path.display()))
}

#[tauri::command]
pub fn terminal_scrollback_forget(node: State<'_, NodeState>, key: String) -> Result<(), String> {
    let path = scrollback_path(&node.api_base()?, &node.api_token(), &key)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("could not forget {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod mobile_development_tests {
    use super::*;

    #[test]
    fn a_mobile_development_run_stays_under_the_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("packages/app")).unwrap();
        assert_eq!(mobile_development_cwd(root, None).unwrap(), root);
        assert_eq!(
            mobile_development_cwd(root, Some(root.join("packages/app").display().to_string()))
                .unwrap(),
            root.join("packages/app")
        );
        assert!(
            mobile_development_cwd(root, Some("/tmp".into())).is_err(),
            "outside the checkout"
        );
        assert!(
            mobile_development_cwd(
                root,
                Some(root.join("packages/../../elsewhere").display().to_string())
            )
            .is_err(),
            "a parent step is refused"
        );
        assert!(
            mobile_development_cwd(root, Some(root.join("missing").display().to_string())).is_err()
        );
    }

    #[test]
    fn a_mobile_development_run_belongs_to_a_checkout() {
        let err = mobile_development_launch("http://127.0.0.1:1", "t", Scope::Goal, "g1", "AAAA")
            .unwrap_err();
        assert!(err.contains("checkout"), "{err}");
    }
}
