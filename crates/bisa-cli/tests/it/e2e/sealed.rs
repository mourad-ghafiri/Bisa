//! The sealed workspace every journey runs in: the real `bisa` binary over a
//! workspace of the test's own, with nothing of this machine in reach.
//!
//! The environment of every process a journey starts is **cleared and built
//! again from a short list** — a home and a temporary folder of the test's
//! own, a `PATH` that finds the scripted agent, `git` and `sh` and nothing
//! else, git isolated from the machine's configuration. No token, no proxy,
//! no agent socket, no real harness and no code host CLI is in it. The
//! workspace keeps its keys in files (`--file-keys`), never a keychain.
//!
//! The agents run on the **scripted agent** (`tests/scripted_agent.rs`),
//! found under the name of an ACP target the platform already knows, so the
//! daemon launches it through the ACP adapter as it would a person's
//! harness. What it does is the script the journey wrote beside it; what it
//! was told is the record the journey reads back.
//!
//! The daemon is the journey's own child. It is asked to stop the way a
//! supervisor asks ([`Sealed::stop`]), so its goodbye is said; a crash is
//! staged by ending it where it stands ([`Sealed::crash`]). A daemon still
//! up when the fixture is dropped — a journey that failed — is ended there.
//!
//! Nothing here deletes anything: the folders are `tempfile`'s, released
//! when the fixture is.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// The program the scripted agent is found as: an ACP target's.
pub const AGENT_PROGRAM: &str = "goose";
/// The harness that program is, as an agent or a step names it.
pub const AGENT_HARNESS: &str = "acp:goose";

/// How long a journey waits for what the daemon does on its own time. A
/// hang guard, not a measure: a loaded machine must not turn a journey red.
const PATIENCE: Duration = Duration::from_secs(90);
/// How long a daemon asked to stop may take to say goodbye.
const GOODBYE: Duration = Duration::from_secs(30);

/// What a journey keeps of the environment it was started in: where the
/// coverage tool writes, and nothing else.
const KEPT: [&str; 3] = [
    "LLVM_PROFILE_FILE",
    "CARGO_LLVM_COV",
    "CARGO_LLVM_COV_TARGET_DIR",
];

pub struct Sealed {
    root: tempfile::TempDir,
    daemon: Option<Child>,
    /// The daemon's stdin, held while it was started leashed
    /// ([`Self::start_leashed`]): letting go of it is the desktop shell
    /// going away, and the daemon's cue to stop.
    leash: Option<std::process::ChildStdin>,
    /// The folder is left behind for a person to read (`BISA_JOURNEY_KEEP=1`).
    kept: bool,
}

impl Sealed {
    /// A workspace, initialised, with the settings that reach outside turned
    /// off — and nothing else: no harness is on its `PATH`, its agents are
    /// as a first launch leaves them. No daemon runs yet.
    pub fn bare() -> Self {
        let mut root = tempfile::tempdir().expect("a folder of the journey's own");
        // `BISA_JOURNEY_KEEP=1` leaves the folder behind — the two nodes'
        // logs among it — for a person reading a failure; its path is said
        // at the end. Nothing of the machine's is in it.
        let kept = std::env::var_os("BISA_JOURNEY_KEEP").is_some_and(|v| v == "1");
        if kept {
            root.disable_cleanup(true);
        }
        let sealed = Self {
            root,
            daemon: None,
            leash: None,
            kept,
        };
        for folder in [sealed.data(), sealed.home(), sealed.agents(), sealed.tmp()] {
            std::fs::create_dir_all(&folder).expect("the journey's folders");
        }
        sealed.ok(&["init"]);
        for (key, value) in [
            // No account is asked for its usage, and no socket is bound
            // beyond the daemon's own. Both are this machine's to say.
            ("harness.usage.reads", "false"),
            ("sync.iroh.enabled", "false"),
            // The shortest the ticker may be: what is looked at on a cadence
            // — a timer, a schedule — is looked at soon.
            ("events.tick_secs", "5"),
        ] {
            sealed.ok(&["settings", "set", "machine", key, value]);
        }
        sealed
    }

    /// [`Self::bare`], with the scripted agent installed and doing what
    /// `script` says, and the two core agents on it.
    pub fn with_script(script: &Value) -> Self {
        let sealed = Self::bare();
        sealed.install_agent(script);
        for agent in ["general-agent", "workflow-agent"] {
            sealed.ok(&["agent", "edit", agent, "--harness", AGENT_HARNESS]);
        }
        sealed
    }

    /// Put the scripted agent where the platform finds a harness — what a
    /// person does when they install one — doing what `script` says.
    pub fn install_agent(&self, script: &Value) {
        self.install_agent_as(AGENT_PROGRAM, script);
    }

    /// [`Self::install_agent`], under another program's name: a harness
    /// with an id of its own (`copilot`, `grok`, `gemini`), found by that adapter's own
    /// probe and started with that adapter's own words. Each name has its
    /// script and its record beside it.
    pub fn install_agent_as(&self, program: &str, script: &Value) {
        let built = Path::new(env!("CARGO_BIN_EXE_scripted-agent"));
        let placed = self.agent_file_of(program, "");
        // A link to the built binary, or a copy where the two folders are on
        // two volumes.
        if std::fs::hard_link(built, &placed).is_err() {
            std::fs::copy(built, &placed).expect("the scripted agent, copied beside its script");
        }
        std::fs::write(
            self.agent_file_of(program, ".script.json"),
            script.to_string(),
        )
        .expect("the agent's script");
    }

    pub fn data(&self) -> PathBuf {
        self.root.path().join("data")
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    fn agents(&self) -> PathBuf {
        self.root.path().join("agents")
    }

    fn tmp(&self) -> PathBuf {
        self.root.path().join("tmp")
    }

    fn agent_file(&self, suffix: &str) -> PathBuf {
        self.agent_file_of(AGENT_PROGRAM, suffix)
    }

    /// A file of the scripted agent placed as `program`.
    fn agent_file_of(&self, program: &str, suffix: &str) -> PathBuf {
        self.agents().join(format!("{program}{suffix}"))
    }

    /// A file of the journey's own — a workflow to record, a document to
    /// give a goal — and where it is.
    pub fn file(&self, name: &str, text: &str) -> PathBuf {
        let folder = self.root.path().join("files");
        std::fs::create_dir_all(&folder).expect("the journey's files");
        let path = folder.join(name);
        std::fs::write(&path, text).expect("the journey's file");
        path
    }

    // --- the binary ----------------------------------------------------------

    /// `bisa`, with the sealed environment and the journey's workspace.
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bisa"));
        command.env_clear();
        for name in KEPT {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let path = format!("{}:/usr/bin:/bin", self.agents().display());
        command
            .env("HOME", self.home())
            .env("TMPDIR", self.tmp())
            .env("PATH", path)
            .env("SHELL", "/bin/sh")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("--data-dir")
            .arg(self.data())
            .arg("--file-keys");
        command
    }

    /// A command line run the way a harness runs its hook: through `sh`,
    /// what it is given on its input, in an environment that holds what the
    /// journey's own does and what `given` adds — and no workspace named,
    /// since a hook is told where the node is and nothing else.
    pub fn hook(&self, line: &str, given: &[(String, String)], input: &str) -> Output {
        use std::io::Write as _;
        let mut command = Command::new("/bin/sh");
        command.env_clear();
        for name in KEPT {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .env("HOME", self.home())
            .env("TMPDIR", self.tmp())
            .env("PATH", "/usr/bin:/bin")
            .env("SHELL", "/bin/sh")
            .envs(given.iter().map(|(name, value)| (name, value)))
            .args(["-c", line])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut hook = command.spawn().expect("the hook runs");
        hook.stdin
            .take()
            .expect("the hook's input")
            .write_all(input.as_bytes())
            .expect("the payload, handed over");
        hook.wait_with_output().expect("the hook ends")
    }

    /// One verb, whatever it answers.
    pub fn bisa(&self, args: &[&str]) -> Output {
        self.command()
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs")
    }

    /// One verb that must succeed.
    pub fn ok(&self, args: &[&str]) -> Output {
        let out = self.bisa(args);
        assert!(
            out.status.success(),
            "bisa {args:?} failed:\nstdout: {}\nstderr: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
            self.what_happened(),
        );
        out
    }

    /// One verb that must succeed, answered as JSON.
    pub fn json(&self, args: &[&str]) -> Value {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let out = self.ok(&all);
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "bisa {args:?} did not answer JSON ({e}): {}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }

    /// One verb that must be refused, asked as a script asks (`--json`), and
    /// what it said. The contract a script reads is held on every refusal a
    /// journey makes: the exit code is 1 — the verb ran and failed — or 2,
    /// when the command line itself could not be read; nothing is written to
    /// stdout, so a reader of JSON never parses half an answer; the words are
    /// on stderr. And the refusal is the verb's own: never for a lock, which
    /// is what a verb that starts an engine beside the node's is answered.
    pub fn refused(&self, args: &[&str]) -> String {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let out = self.bisa(&all);
        let said = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            matches!(out.status.code(), Some(1 | 2)),
            "bisa {args:?} was not refused (exit {:?}):\nstdout: {}\nstderr: {said}\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            self.what_happened(),
        );
        assert!(
            out.stdout.is_empty(),
            "bisa {args:?} failed and still wrote to stdout in --json: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            !said.trim().is_empty(),
            "bisa {args:?} failed without a word"
        );
        assert!(
            !said.contains("another engine holds this workspace"),
            "bisa {args:?} started an engine of its own beside the node's: {said}"
        );
        said
    }

    /// One verb begun and left to go on — a verb that follows a run until it
    /// settles, while the journey does something else. What it said is kept
    /// in a file of its own name; it is ended with the value that holds it.
    pub fn begin(&self, name: &str, args: &[&str]) -> Begun {
        let said = |suffix: &str| {
            std::fs::File::create(self.root.path().join(format!("{name}.{suffix}")))
                .expect("the verb's own words, kept")
        };
        let child = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .stdout(said("out"))
            .stderr(said("err"))
            .spawn()
            .expect("the binary runs");
        Begun(child)
    }

    /// What a begun verb has said so far — its output and its errors, as
    /// the files of its name hold them.
    pub fn said_by(&self, name: &str) -> (String, String) {
        let read = |suffix: &str| {
            std::fs::read_to_string(self.root.path().join(format!("{name}.{suffix}")))
                .unwrap_or_default()
        };
        (read("out"), read("err"))
    }

    // --- the daemon ------------------------------------------------------------

    /// Start `bisa node` and wait for its socket. A daemon that cannot bind
    /// fails the journey in its own words: a run that could not start it
    /// proved nothing.
    pub fn start(&mut self) {
        self.start_with(&[], &[]);
    }

    /// [`Self::start`], with these variables in the daemon's environment
    /// besides the sealed few — what a machine's shell would hand it: a
    /// service's token among them, so the redactor's own reading of the
    /// environment has something to find.
    pub fn start_in(&mut self, environment: &[(&str, &str)]) {
        self.start_with(&[], environment);
    }

    /// [`Self::start`], listening on a loopback port besides its socket —
    /// the address a desktop's terminals give their hooks. Answers the
    /// address as the node itself says it (`GET /node`).
    pub fn start_listening(&mut self, port: u16) -> String {
        self.start_with(&["--listen", &format!("127.0.0.1:{port}")], &[]);
        let (status, node) = self.call("GET", "/node", &[], None);
        assert_eq!(status, 200, "{node}");
        node["listen"]
            .as_str()
            .unwrap_or_else(|| panic!("a node that listens says where: {node}"))
            .to_string()
    }

    fn start_with(&mut self, listening: &[&str], environment: &[(&str, &str)]) {
        self.spawn_daemon(listening, environment, false);
    }

    /// [`Self::start`] the way the desktop shell starts its node: `--json`,
    /// so the boot's phases come as lines on stdout, and leashed — its stdin
    /// a pipe this harness holds, `BISA_STOP_ON_STDIN_CLOSE=1` — so the
    /// daemon stops when the pipe closes ([`Self::drop_leash`]).
    pub fn start_leashed(&mut self) {
        self.spawn_daemon(&[], &[("BISA_STOP_ON_STDIN_CLOSE", "1")], true);
    }

    fn spawn_daemon(&mut self, listening: &[&str], environment: &[(&str, &str)], leashed: bool) {
        assert!(self.daemon.is_none(), "the daemon is already up");
        let said = |name: &str| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.root.path().join(name))
                .expect("the daemon's own words, kept")
        };
        let mut command = self.command();
        command.envs(environment.iter().copied());
        if leashed {
            command.arg("--json");
        }
        let mut daemon = command
            .arg("node")
            .args(listening)
            .stdin(if leashed {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(said("daemon.out"))
            .stderr(said("daemon.err"))
            .spawn()
            .expect("the daemon starts");
        if leashed {
            self.leash = daemon.stdin.take();
        }
        let deadline = Instant::now() + PATIENCE;
        while !self.socket().exists() {
            let ended = daemon.try_wait().expect("the daemon's state");
            assert!(
                ended.is_none() && Instant::now() < deadline,
                "the daemon's socket never bound (it ended: {ended:?}):\n{}",
                self.what_happened(),
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        self.daemon = Some(daemon);
    }

    /// Where the daemon's socket is: the preferred path, or — under a long
    /// folder — the short one its pointer names.
    pub fn socket(&self) -> PathBuf {
        let preferred = bisa_store::Paths::new(self.data()).node_socket();
        if preferred.exists() {
            return preferred;
        }
        std::fs::read_to_string(bisa_node::pointer_path(&preferred))
            .map(|p| PathBuf::from(p.trim()))
            .unwrap_or(preferred)
    }

    /// The daemon's process id — what the engine lock records as its holder.
    pub fn pid(&self) -> u32 {
        self.daemon.as_ref().expect("a daemon up").id()
    }

    /// Ask the daemon to stop, the way a supervisor does, and wait for it:
    /// it ends by itself, well, and leaves no socket behind.
    pub fn stop(&mut self) {
        let mut daemon = self.daemon.take().expect("a daemon to stop");
        let socket = self.socket();
        let pid = rustix::process::Pid::from_child(&daemon);
        rustix::process::kill_process(pid, rustix::process::Signal::TERM)
            .expect("the daemon takes the request");
        let deadline = Instant::now() + GOODBYE;
        let status = loop {
            if let Some(status) = daemon.try_wait().expect("the daemon's state") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "the daemon was asked to stop and did not:\n{}",
                self.what_happened(),
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(
            status.success(),
            "a daemon asked to stop ends well, not {status}:\n{}",
            self.what_happened(),
        );
        assert!(
            !socket.exists(),
            "the daemon left its socket behind at {}",
            socket.display()
        );
    }

    /// Let go of the leash — the desktop shell going away, a Force Quit
    /// among its ways — and wait for the daemon to stop on its own: it ends
    /// well and leaves no socket behind, as a stop asks of it.
    pub fn drop_leash(&mut self) {
        let mut daemon = self.daemon.take().expect("a daemon to let go of");
        let socket = self.socket();
        drop(self.leash.take().expect("a daemon started leashed"));
        let deadline = Instant::now() + GOODBYE;
        let status = loop {
            if let Some(status) = daemon.try_wait().expect("the daemon's state") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "the daemon's leash was dropped and it did not stop:\n{}",
                self.what_happened(),
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(
            status.success(),
            "a daemon whose parent went ends well, not {status}:\n{}",
            self.what_happened(),
        );
        assert!(
            !socket.exists(),
            "the daemon left its socket behind at {}",
            socket.display()
        );
    }

    /// What the daemon wrote to its stdout so far — under `--json`, one JSON
    /// object a line.
    pub fn daemon_out(&self) -> String {
        std::fs::read_to_string(self.root.path().join("daemon.out")).unwrap_or_default()
    }

    /// End the daemon where it stands, as a crash or a power cut would: no
    /// goodbye, nothing flushed that was not already on disk.
    pub fn crash(&mut self) {
        let mut daemon = self.daemon.take().expect("a daemon to end");
        end_abruptly(&mut daemon);
        // The socket a killed daemon leaves behind: nothing listens there,
        // and the next [`Self::start`] waits for a socket to appear — left
        // in place, it would return before the new daemon has opened the
        // workspace, and a verb run then would embed an engine of its own
        // over the same files. A real node removes a stale socket when it
        // binds; the harness removes it here so the wait means something.
        if let Err(e) = std::fs::remove_file(self.socket()) {
            assert!(
                e.kind() == std::io::ErrorKind::NotFound,
                "the crashed daemon's socket could not be removed: {e}"
            );
        }
    }

    // --- the node, called --------------------------------------------------------

    /// One call to the node over its socket, as a program on this machine
    /// makes one: under the control-plane token the daemon wrote for this
    /// workspace, with the headers given. The status, and the body as JSON
    /// (`null` when it is none).
    pub fn call(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> (u16, Value) {
        let token = std::fs::read_to_string(bisa_store::Paths::new(self.data()).token_file())
            .expect("the daemon wrote the workspace's token");
        let bearer = format!("Bearer {}", token.trim());
        let mut all = vec![("authorization", bearer.as_str())];
        all.extend_from_slice(headers);
        self.call_with(method, path, &all, body)
    }

    /// [`Self::call`] with exactly the headers given — no token unless one
    /// is among them: a caller from outside.
    pub fn call_with(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> (u16, Value) {
        use http_body_util::BodyExt as _;
        let socket = self.socket();
        let sent = body.map(Value::to_string).unwrap_or_default();
        let mut request = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost");
        if body.is_some() {
            request = request.header(hyper::header::CONTENT_TYPE, "application/json");
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let request = request
            .body(http_body_util::Full::new(hyper::body::Bytes::from(sent)))
            .expect("a request");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime for one call");
        runtime.block_on(async move {
            let stream = tokio::net::UnixStream::connect(&socket)
                .await
                .unwrap_or_else(|e| panic!("no node at {}: {e}", socket.display()));
            let (mut sender, connection) =
                hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
                    .await
                    .expect("the node speaks HTTP");
            tokio::spawn(connection);
            let answer = sender.send_request(request).await.expect("an answer");
            let status = answer.status().as_u16();
            let bytes = answer
                .into_body()
                .collect()
                .await
                .expect("the answer's body")
                .to_bytes();
            (
                status,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            )
        })
    }

    /// Open the node's event stream and keep it open, as a desktop does:
    /// every frame it carries is kept, and so is the stream's end.
    pub fn listen(&self) -> Listening {
        let token = std::fs::read_to_string(bisa_store::Paths::new(self.data()).token_file())
            .expect("the daemon wrote the workspace's token");
        let socket = self.socket();
        let heard = std::sync::Arc::new(std::sync::Mutex::new(Heard::default()));
        let keeps = std::sync::Arc::clone(&heard);
        let reader = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for one stream");
            runtime.block_on(follow(socket, token.trim().to_string(), keeps));
        });
        let listening = Listening {
            heard,
            reader: Some(reader),
        };
        self.until("the stream to be open", || {
            listening.heard().open.then_some(())
        });
        listening
    }

    // --- what the agent was told -------------------------------------------------

    /// Every fact the scripted agent kept, in order.
    pub fn record(&self) -> Vec<Value> {
        std::fs::read_to_string(self.agent_file(".record.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// The facts of one kind.
    pub fn recorded(&self, event: &str) -> Vec<Value> {
        self.record()
            .into_iter()
            .filter(|fact| fact["event"] == event)
            .collect()
    }

    /// The facts of one kind kept by the scripted agent placed as `program`
    /// ([`Self::install_agent_as`]), in order.
    pub fn recorded_by(&self, program: &str, event: &str) -> Vec<Value> {
        std::fs::read_to_string(self.agent_file_of(program, ".record.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|fact| fact["event"] == event)
            .collect()
    }

    // --- waiting -----------------------------------------------------------------

    /// Ask until `probe` answers, or fail saying what was waited for and
    /// what the daemon and the agent had said by then.
    pub fn until<T>(&self, what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(found) = probe() {
                return found;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}:\n{}",
                self.what_happened(),
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The end of what the daemon and the agent said, for a failure to show.
    fn what_happened(&self) -> String {
        let tail = |path: PathBuf| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let from = lines.len().saturating_sub(30);
            format!("--- {}\n{}", path.display(), lines[from..].join("\n"))
        };
        // The node's own log — its newest two files, whichever family.
        let mut logs: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        let mut folders = vec![self.data().join("logs")];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else if path.extension().is_some_and(|e| e == "jsonl" || e == "log") {
                    let at = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    logs.push((at, path));
                }
            }
        }
        logs.sort();
        let newest: Vec<String> = logs
            .iter()
            .rev()
            .take(2)
            .map(|(_, p)| tail(p.clone()))
            .collect();
        [
            tail(self.root.path().join("daemon.err")),
            tail(self.root.path().join("daemon.out")),
            tail(self.agent_file(".record.jsonl")),
            tail(self.agent_file(".mcp.log")),
        ]
        .into_iter()
        .chain(newest)
        .collect::<Vec<_>>()
        .join("\n")
    }
}

/// What an open event stream carried, and whether it is still open.
#[derive(Default, Clone)]
pub struct Heard {
    /// The node answered and the stream is being read.
    pub open: bool,
    /// Every frame, in the order it came.
    pub frames: Vec<Value>,
    /// The stream ended: the node closed it, or went away.
    pub ended: bool,
}

/// An event stream kept open ([`Sealed::listen`]).
pub struct Listening {
    heard: std::sync::Arc<std::sync::Mutex<Heard>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Listening {
    pub fn heard(&self) -> Heard {
        self.heard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// The engine events heard so far, by their type.
    pub fn engine_events(&self) -> Vec<String> {
        self.heard()
            .frames
            .iter()
            .filter(|frame| frame["stream"] == "engine")
            .filter_map(|frame| frame["payload"]["payload"]["type"].as_str())
            .map(str::to_string)
            .collect()
    }

    /// Wait for the stream's reader to be done: the stream ended.
    pub fn ended(mut self) -> Heard {
        if let Some(reader) = self.reader.take() {
            reader.join().expect("the stream's reader ends by itself");
        }
        self.heard()
    }
}

/// Read the node's event stream until it ends, keeping what it carries.
async fn follow(socket: PathBuf, token: String, heard: std::sync::Arc<std::sync::Mutex<Heard>>) {
    use http_body_util::BodyExt as _;
    let keep = |change: &dyn Fn(&mut Heard)| {
        change(
            &mut heard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    };
    let Ok(stream) = tokio::net::UnixStream::connect(&socket).await else {
        keep(&|h| h.ended = true);
        return;
    };
    let Ok((mut sender, connection)) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream)).await
    else {
        keep(&|h| h.ended = true);
        return;
    };
    tokio::spawn(connection);
    let request = hyper::Request::builder()
        .method("GET")
        .uri("/events")
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(http_body_util::Empty::<hyper::body::Bytes>::new())
        .expect("a request");
    let Ok(answer) = sender.send_request(request).await else {
        keep(&|h| h.ended = true);
        return;
    };
    if !answer.status().is_success() {
        keep(&|h| h.ended = true);
        return;
    }
    keep(&|h| h.open = true);
    let mut body = answer.into_body();
    let mut frames = bisa_http::SseFrames::default();
    while let Some(Ok(frame)) = body.frame().await {
        let Some(data) = frame.data_ref() else {
            continue;
        };
        if frames.push(data).is_err() {
            break;
        }
        while let Some(event) = frames.next_event() {
            if let Ok(value) = serde_json::from_str::<Value>(&event.data) {
                keep(&|h| h.frames.push(value.clone()));
            }
        }
    }
    keep(&|h| h.ended = true);
}

/// A verb that was begun and goes on ([`Sealed::begin`]).
pub struct Begun(Child);

impl Begun {
    /// Whether it ended by itself, and how.
    pub fn ended(&mut self) -> Option<std::process::ExitStatus> {
        self.0.try_wait().expect("the verb's state")
    }
}

impl Drop for Begun {
    fn drop(&mut self) {
        end_abruptly(&mut self.0);
    }
}

impl Drop for Sealed {
    fn drop(&mut self) {
        if let Some(mut daemon) = self.daemon.take() {
            end_abruptly(&mut daemon);
        }
        if self.kept {
            eprintln!("journey folder kept: {}", self.root.path().display());
        }
    }
}

/// End a process of the journey's own and reap it; one already gone is gone.
fn end_abruptly(process: &mut Child) {
    let _already_gone = process.kill();
    let _reaped = process.wait();
}
