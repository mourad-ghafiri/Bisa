//! A scripted agent: a harness that does what a script says, for the
//! journeys under `tests/it/e2e/`. It speaks the Agent Client Protocol over
//! stdio (JSON-RPC, one message a line) the way the ACP adapter expects a
//! real agent to, so the daemon under test launches it, prompts it and hears
//! it through the code a person's harness goes through — no test door.
//!
//! What it does when prompted is written by the test, in a JSON file beside
//! the binary (`<binary>.script.json`): say these words, call these tools of
//! the MCP servers it was handed, write these files in its working folder,
//! use a tool of its own — an edit of one file, a command that leaves files
//! behind (it runs nothing: the files are what the command is said to have
//! left) — ask before a tool, hold the turn until it is cancelled. A turn
//! whose `tools` is the word `"every"` calls every tool the platform's
//! server lists, each with nothing in its hands — what an agent that tries
//! its whole menu does.
//!
//! A tool call is told the way the protocol tells one: announced once with
//! its title, its kind and its input (`tool_call`), then spoken of by its id
//! alone — the question before it runs, its progress, its end — since every
//! field of a later message but the id is optional. The MCP servers
//! are started as a harness starts them — the command, the arguments and the
//! environment of `session/new`, nothing added — asked for their tools as a
//! harness asks (`tools/list`, kept with the fact that the server started),
//! and spoken to by the small client below. What it was told is kept in
//! `<binary>.record.jsonl`, one fact a line, for the test to read back.
//!
//! Several run at once where a run fans out — each session is a process of
//! its own — and nothing here is shared that two could write at once: a
//! fact is one appended line, written whole; a session has a file of its
//! own, and making that file is what makes its id its own; a turn that may
//! be taken once is taken by whoever makes its mark first.
//!
//! The protocol as read on 2026-09-29:
//! <https://agentclientprotocol.com/protocol/session-setup> (`session/new`,
//! and `session/load`: the conversation replayed as `session/update`, then
//! an empty result), <https://agentclientprotocol.com/protocol/prompt-turn>
//! (`session/prompt`, its updates, `session/request_permission`,
//! `session/cancel` answered `cancelled`) and
//! <https://agentclientprotocol.com/protocol/session-config-options>
//! (`configOptions`, `session/set_config_option` answered with the whole
//! list); and, read on 2026-09-30,
//! <https://agentclientprotocol.com/protocol/tool-calls> (`tool_call`,
//! `tool_call_update`, the kinds and the statuses).
//!
//! It is found under whatever name a journey places it — an ACP target's, or
//! a harness with an id of its own (`copilot`, `grok`) — and the words it was
//! started with are the first fact of its record, so a journey reads the
//! command line an adapter built. Two words it answers without a script's
//! turn, as the CLIs it stands in for do: `--version`, with a version, and
//! `models`, with the rows `grok models` prints. A script that names
//! `models` makes its sessions choose their model (a `model` config option,
//! the first listed the one a session opens on), and `model_efforts` gives a
//! model levels of its own — what lets a journey see that the effort was
//! fitted to the model that was set, and not to the one the session opened on.
//!
//! A binary of this package, never shipped: the distribution builds
//! `--bin bisa` alone.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// The protocol revision the MCP client asks for; the server answers the one
/// it speaks.
const MCP_REVISION: &str = "2025-06-18";

/// The platform's own server among the ones a session is handed.
const PLATFORM_SERVER: &str = "bisa";

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    // A probe: answered before any script is read, as a CLI answers it.
    if words.iter().any(|word| word == "--version") {
        println!("scripted-agent {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let beside = Beside::the_binary();
    if words.first().map(String::as_str) == Some("models") {
        print_models(&beside.script());
        return;
    }
    let mut agent = Agent {
        script: beside.script(),
        beside,
        clients: BTreeMap::new(),
        input: std::io::stdin().lock().lines(),
    };
    agent.record(
        json!({ "event": "started", "args": std::env::args().skip(1).collect::<Vec<_>>() }),
    );
    while let Some(message) = agent.next_message() {
        agent.handle(message);
    }
    agent.close();
}

/// The models the script names, as `grok models` prints them: who is signed
/// in, the default, then a row a model — the default's marked.
fn print_models(script: &Value) {
    let models = listed_models(script);
    println!("You are using XAI_API_KEY.");
    println!();
    if let Some(default) = models.first() {
        println!("Default model: {default}");
        println!();
    }
    println!("Available models:");
    for (n, model) in models.iter().enumerate().rev() {
        // Printed last first, so a reader that took the first row for the
        // default would be wrong.
        if n == 0 {
            println!("  * {model} (default)");
        } else {
            println!("  - {model}");
        }
    }
}

/// The models a script says its sessions choose among; none when it says
/// nothing, and the sessions then offer no model option at all.
fn listed_models(script: &Value) -> Vec<String> {
    script["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| model.as_str().map(str::to_string))
        .collect()
}

/// The files beside the binary: the script the test wrote, the record it
/// reads back, and what this agent keeps between two processes.
struct Beside {
    binary: PathBuf,
}

impl Beside {
    fn the_binary() -> Self {
        let binary =
            std::env::current_exe().unwrap_or_else(|e| fail(&format!("no path of its own: {e}")));
        Self { binary }
    }

    fn file(&self, suffix: &str) -> PathBuf {
        let mut name = self.binary.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    }

    fn script(&self) -> Value {
        let path = self.file(".script.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| fail(&format!("no script at {}: {e}", path.display())));
        serde_json::from_str(&text).unwrap_or_else(|e| {
            fail(&format!(
                "the script at {} is not JSON: {e}",
                path.display()
            ))
        })
    }

    /// A folder beside the binary, made when it is first asked for.
    fn folder(&self, suffix: &str) -> PathBuf {
        let folder = self.file(suffix);
        if let Err(e) = std::fs::create_dir_all(&folder) {
            fail(&format!("no folder at {}: {e}", folder.display()));
        }
        folder
    }

    /// A file that is there only if this call made it: whoever makes it
    /// first has it, however many ask at once.
    fn made(path: &Path, holding: &str) -> bool {
        let made = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path);
        match made {
            Ok(mut file) => {
                if let Err(e) = file.write_all(holding.as_bytes()) {
                    fail(&format!("{} cannot be written: {e}", path.display()));
                }
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(e) => fail(&format!("{} cannot be made: {e}", path.display())),
        }
    }

    /// What outlives a process, a session at a time: a revived session is a
    /// new process, and must know the conversation it is asked to load. One
    /// process serves a session at a time, so its file has one writer.
    fn session(&self, id: &str) -> PathBuf {
        self.folder(".sessions").join(format!("{id}.json"))
    }

    /// A new session, under the first number nobody took.
    fn begin_session(&self) -> String {
        (1u64..)
            .map(|n| format!("scripted-{n}"))
            .find(|id| Self::made(&self.session(id), &json!({ "history": [] }).to_string()))
            .unwrap_or_else(|| fail("no number is left for a session"))
    }

    fn known(&self, session: &str) -> Option<Value> {
        let text = std::fs::read_to_string(self.session(session)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// A session as it is known, changed and kept.
    fn change(&self, session: &str, change: impl FnOnce(&mut Value)) {
        let mut known = self
            .known(session)
            .unwrap_or_else(|| json!({ "history": [] }));
        change(&mut known);
        let path = self.session(session);
        if let Err(e) = std::fs::write(&path, known.to_string()) {
            fail(&format!("{} cannot be written: {e}", path.display()));
        }
    }

    /// One use of a turn that may be taken `times` times, when one is left.
    fn take(&self, place: &str, times: u64) -> bool {
        let marks = self.folder(".taken");
        (0..times).any(|n| Self::made(&marks.join(format!("{place}.{n}")), ""))
    }
}

struct Agent {
    beside: Beside,
    script: Value,
    /// The MCP servers started so far, by session and name.
    clients: BTreeMap<(String, String), McpClient>,
    input: std::io::Lines<std::io::StdinLock<'static>>,
}

/// How a turn ended.
enum TurnEnd {
    Stop(String),
    Cancelled,
}

impl Agent {
    // --- the wire ----------------------------------------------------------

    fn next_message(&mut self) -> Option<Value> {
        loop {
            let line = self.input.next()?.ok()?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str(line) {
                Ok(value) => return Some(value),
                Err(e) => self
                    .record(json!({ "event": "unreadable", "line": line, "error": e.to_string() })),
            }
        }
    }

    fn send(&self, message: Value) {
        let mut out = std::io::stdout().lock();
        let sent = writeln!(out, "{message}").and_then(|()| out.flush());
        if sent.is_err() {
            // Nobody reads any more: the client ended, by itself or cut off.
            // The agent goes with it, and says so in its record.
            self.record(json!({ "event": "ended", "why": "the client is gone" }));
            std::process::exit(0);
        }
    }

    fn answer(&self, id: &Value, result: Value) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn refuse(&self, id: &Value, code: i64, message: &str) {
        self.send(
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        );
    }

    fn update(&self, session: &str, update: Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": { "sessionId": session, "update": update },
        }));
    }

    fn record(&self, mut fact: Value) {
        if let Some(object) = fact.as_object_mut() {
            object.insert("pid".into(), json!(std::process::id()));
        }
        let path = self.beside.file(".record.jsonl");
        // One line, one write: an append is whole or not there, so the facts
        // of several agents never run into one another.
        let line = format!("{fact}\n");
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if let Err(e) = written {
            fail(&format!(
                "the record at {} cannot be written: {e}",
                path.display()
            ));
        }
    }

    // --- the protocol ------------------------------------------------------

    fn handle(&mut self, message: Value) {
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // An answer nobody is waiting for: a permission's, after its turn.
            return;
        };
        match (method, id) {
            ("initialize", Some(id)) => {
                self.record(json!({ "event": "initialize", "params": params }));
                self.answer(
                    &id,
                    json!({
                        "protocolVersion": 1,
                        "agentCapabilities": { "loadSession": true },
                    }),
                );
            }
            ("session/new", Some(id)) => {
                let session = self.beside.begin_session();
                self.open(&session, &params, "session_new");
                self.answer(
                    &id,
                    json!({ "sessionId": session, "configOptions": self.config_options(&session) }),
                );
            }
            ("session/load", Some(id)) => {
                let session = text(&params["sessionId"]);
                let Some(known) = self.beside.known(&session) else {
                    self.record(json!({ "event": "load_refused", "session": session }));
                    self.refuse(&id, -32002, &format!("no session {session}"));
                    return;
                };
                self.open(&session, &params, "session_load");
                // The conversation as it was, then the answer — with no id.
                for said in known["history"].as_array().into_iter().flatten() {
                    let kind = match said["by"].as_str() {
                        Some("agent") => "agent_message_chunk",
                        _ => "user_message_chunk",
                    };
                    self.update(
                        &session,
                        json!({ "sessionUpdate": kind, "content": { "type": "text", "text": said["text"] } }),
                    );
                }
                self.answer(&id, json!({}));
            }
            ("session/set_config_option", Some(id)) => {
                let session = text(&params["sessionId"]);
                self.record(json!({
                    "event": "config",
                    "session": session,
                    "config_id": params["configId"],
                    "value": params["value"],
                }));
                // A model is one the session offers, or the set is refused —
                // as an agent refuses a model its account lacks.
                let value = text(&params["value"]);
                if params["configId"] == "model" && !listed_models(&self.script).contains(&value) {
                    self.refuse(&id, -32602, &format!("model not found: {value}"));
                    return;
                }
                let kept = if params["configId"] == "model" {
                    "model"
                } else {
                    "effort"
                };
                self.beside
                    .change(&session, |known| known[kept] = params["value"].clone());
                self.answer(
                    &id,
                    json!({ "configOptions": self.config_options(&session) }),
                );
            }
            ("session/prompt", Some(id)) => {
                let session = text(&params["sessionId"]);
                let prompt = prompt_text(&params["prompt"]);
                let stop = match self.turn(&session, &prompt) {
                    TurnEnd::Stop(reason) => reason,
                    TurnEnd::Cancelled => "cancelled".to_string(),
                };
                self.answer(&id, json!({ "stopReason": stop }));
            }
            ("session/cancel", None) => {
                // Between two turns there is nothing to cancel.
                self.record(json!({ "event": "cancel", "session": params["sessionId"], "during": "nothing" }));
            }
            (other, Some(id)) => self.refuse(&id, -32601, &format!("no method {other}")),
            (_, None) => {}
        }
    }

    /// A session as it was opened or loaded: where it runs, what it was
    /// handed, which of the platform's scopes it serves.
    fn open(&mut self, session: &str, params: &Value, event: &str) {
        let servers = params["mcpServers"].clone();
        let scope = Scope::of(&servers);
        self.record(json!({
            "event": event,
            "session": session,
            "cwd": params["cwd"],
            "servers": servers,
            "scope": scope.kind,
            "agent": scope.named("agent"),
        }));
        self.beside.change(session, |known| {
            known["cwd"] = params["cwd"].clone();
            known["servers"] = servers;
        });
    }

    /// What the session offers to be set: its model, when the script names
    /// the models it chooses among, and how hard that model works — the
    /// levels the script gives the model, else the script's, else three.
    fn config_options(&self, session: &str) -> Value {
        let known = self.beside.known(session).unwrap_or(Value::Null);
        let models = listed_models(&self.script);
        let model = known["model"]
            .as_str()
            .map(str::to_string)
            .or_else(|| models.first().cloned());
        let offered = model
            .as_deref()
            .and_then(|model| self.script["model_efforts"][model].as_array())
            .or_else(|| self.script["efforts"].as_array())
            .cloned()
            .unwrap_or_else(|| vec![json!("low"), json!("medium"), json!("high")]);
        let current = known["effort"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| "medium".to_string());
        let choices = |values: &[Value]| -> Vec<Value> {
            values
                .iter()
                .map(|v| json!({ "value": v, "name": v }))
                .collect()
        };
        let mut options = Vec::new();
        if let Some(model) = model {
            let listed: Vec<Value> = models.iter().map(|m| json!(m)).collect();
            options.push(json!({
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": model,
                "options": choices(&listed),
            }));
        }
        options.push(json!({
            "id": "effort",
            "name": "Effort",
            "category": "thought_level",
            "type": "select",
            "currentValue": current,
            "options": choices(&offered),
        }));
        Value::Array(options)
    }

    // --- a turn ------------------------------------------------------------

    fn turn(&mut self, session: &str, prompt: &str) -> TurnEnd {
        let known = self.beside.known(session).unwrap_or(Value::Null);
        let scope = Scope::of(&known["servers"]);
        let chosen = self.choose(&scope, prompt);
        self.record(json!({
            "event": "prompt",
            "session": session,
            "scope": scope.kind,
            "agent": scope.named("agent"),
            "text": prompt,
            "turn": chosen.as_ref().map(|(place, _)| place.clone()),
        }));
        self.remember(session, "person", prompt);
        let Some((_, turn)) = chosen else {
            return TurnEnd::Stop("end_turn".into());
        };

        if let Some(thought) = turn["think"].as_str() {
            self.update(
                session,
                json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": thought } }),
            );
        }
        let cwd = PathBuf::from(text(&known["cwd"]));
        for file in turn["write"].as_array().into_iter().flatten() {
            self.write(session, &cwd, file);
        }
        let calls: Vec<Value> = match &turn["tools"] {
            Value::String(word) if word == "every" => self.every_tool(session, &known),
            listed => listed.as_array().cloned().unwrap_or_default(),
        };
        for (n, call) in calls.iter().enumerate() {
            if let Some(end) = self.call(session, &known, &scope, prompt, n, call) {
                return end;
            }
        }
        for words in turn["say"].as_array().into_iter().flatten() {
            let words = text(&filled(words, &scope, &known, prompt));
            self.update(
                session,
                json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": words } }),
            );
            self.remember(session, "agent", &words);
        }
        if turn["hold"].as_bool() == Some(true) {
            return self.hold(session);
        }
        TurnEnd::Stop(turn["stop"].as_str().unwrap_or("end_turn").to_string())
    }

    /// The first turn of the script that fits: its scope, its agent and its
    /// words, where it names any — and, where it may be taken so many times,
    /// one of its uses still free, taken in the asking.
    fn choose(&self, scope: &Scope, prompt: &str) -> Option<(String, Value)> {
        let fits = |turn: &Value| {
            let scoped = turn["scope"].as_str().is_none_or(|s| s == scope.kind);
            let agent = turn["agent"]
                .as_str()
                .is_none_or(|a| scope.named("agent").as_deref() == Some(a));
            let worded = turn["when"].as_str().is_none_or(|w| prompt.contains(w));
            scoped && agent && worded
        };
        let turns = self.script["turns"].as_array().into_iter().flatten();
        for (n, turn) in turns.enumerate() {
            let place = format!("turns[{n}]");
            let left = turn["times"]
                .as_u64()
                .is_none_or(|times| fits(turn) && self.beside.take(&place, times));
            if fits(turn) && left {
                return Some((place, turn.clone()));
            }
        }
        let otherwise = &self.script["otherwise"];
        otherwise
            .is_object()
            .then(|| ("otherwise".to_string(), otherwise.clone()))
    }

    fn remember(&mut self, session: &str, by: &str, words: &str) {
        self.beside.change(session, |known| {
            if let Some(history) = known["history"].as_array_mut() {
                history.push(json!({ "by": by, "text": words }));
            }
        });
    }

    /// A file written in the session's own folder, and nowhere else.
    /// Whether it failed.
    fn write(&mut self, session: &str, cwd: &Path, file: &Value) -> bool {
        let relative = PathBuf::from(text(&file["path"]));
        let inside = relative.is_relative()
            && relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)));
        let outcome = if !inside {
            Err("a path outside the session's folder".to_string())
        } else {
            let path = cwd.join(&relative);
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, text(&file["content"])))
                .map_err(|e| e.to_string())
        };
        let failed = outcome.is_err();
        self.record(json!({
            "event": "write",
            "session": session,
            "path": file["path"],
            "error": outcome.err(),
        }));
        failed
    }

    /// One tool — a server's, or one of the agent's own (`own`: `edit`, a
    /// file written; `command`, the files it `leaves`) — announced, asked
    /// about where the script says so, done, its answer kept. `Some` when
    /// the turn ends here.
    fn call(
        &mut self,
        session: &str,
        known: &Value,
        scope: &Scope,
        prompt: &str,
        n: usize,
        call: &Value,
    ) -> Option<TurnEnd> {
        let name = text(&call["name"]);
        let server = call["server"]
            .as_str()
            .unwrap_or(PLATFORM_SERVER)
            .to_string();
        let arguments = filled(&call["arguments"], scope, known, prompt);
        let call_id = format!("call-{n}");
        let own = call["own"].as_str();
        let ask = call.get("ask").filter(|a| !a.is_null());
        // What the call is announced as: what the question is about, where
        // the script asks about something else than the tool it then calls.
        let asked = |field: &str| ask.and_then(|a| a[field].as_str());
        let title = asked("title").unwrap_or(&name).to_string();
        let kind = asked("kind")
            .or(call["kind"].as_str())
            .unwrap_or(match (own, ask) {
                (Some("edit"), _) => "edit",
                (Some(_), _) | (None, Some(_)) => "execute",
                (None, None) => "other",
            })
            .to_string();
        let raw_input = ask
            .and_then(|a| a.get("raw_input"))
            .cloned()
            .unwrap_or_else(|| arguments.clone());
        self.update(
            session,
            json!({
                "sessionUpdate": "tool_call",
                "toolCallId": call_id,
                "title": title,
                "kind": kind,
                "status": if ask.is_some() { "pending" } else { "in_progress" },
                "rawInput": raw_input,
            }),
        );
        // From here on the call is spoken of by its id alone.
        let moved = |status: &str| json!({ "sessionUpdate": "tool_call_update", "toolCallId": call_id, "status": status });
        if ask.is_some() {
            match self.ask(session, &call_id, &title) {
                Asked::Allowed => self.update(session, moved("in_progress")),
                Asked::Refused => {
                    self.record(
                        json!({ "event": "tool_refused", "session": session, "name": name }),
                    );
                    self.update(session, moved("failed"));
                    return None;
                }
                Asked::Cancelled => return Some(TurnEnd::Cancelled),
            }
        }
        let cwd = PathBuf::from(text(&known["cwd"]));
        let failed = match own {
            Some("edit") => self.write(session, &cwd, &arguments),
            // Every file it leaves is written, whichever failed.
            Some(_) => {
                let mut failed = false;
                for file in call["leaves"].as_array().into_iter().flatten() {
                    failed |= self.write(session, &cwd, file);
                }
                failed
            }
            None => {
                let answered = self
                    .client(session, &server, known)
                    .and_then(|client| client.call(&name, &arguments));
                let (result, failed) = match answered {
                    Ok(result) => {
                        let failed = result["isError"].as_bool() == Some(true);
                        (result, failed)
                    }
                    Err(e) => (json!({ "error": e }), true),
                };
                self.record(json!({
                    "event": "tool",
                    "session": session,
                    "server": server,
                    "name": name,
                    "arguments": arguments,
                    "result": result,
                    "failed": failed,
                }));
                failed
            }
        };
        self.update(session, moved(if failed { "failed" } else { "completed" }));
        None
    }

    /// Ask before a tool runs, and wait for the answer — or for a cancel.
    /// The call was announced: the question names it by its id.
    fn ask(&mut self, session: &str, call_id: &str, name: &str) -> Asked {
        let request = json!(format!("permission-{call_id}"));
        self.send(json!({
            "jsonrpc": "2.0",
            "id": request,
            "method": "session/request_permission",
            "params": {
                "sessionId": session,
                "toolCall": { "toolCallId": call_id },
                "options": [
                    { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
                    { "optionId": "reject", "name": "Reject", "kind": "reject_once" },
                ],
            },
        }));
        let outcome = loop {
            let Some(message) = self.next_message() else {
                break Asked::Cancelled;
            };
            if message["method"] == json!("session/cancel") {
                self.record(
                    json!({ "event": "cancel", "session": session, "during": "a question" }),
                );
                break Asked::Cancelled;
            }
            if message.get("id") != Some(&request) {
                self.handle(message);
                continue;
            }
            let chosen = &message["result"]["outcome"];
            break match (chosen["outcome"].as_str(), chosen["optionId"].as_str()) {
                (Some("selected"), Some("allow")) => Asked::Allowed,
                (Some("selected"), _) => Asked::Refused,
                _ => Asked::Cancelled,
            };
        };
        self.record(json!({
            "event": "permission",
            "session": session,
            "tool": name,
            "outcome": match outcome {
                Asked::Allowed => "allowed",
                Asked::Refused => "refused",
                Asked::Cancelled => "cancelled",
            },
        }));
        outcome
    }

    /// A turn that goes on until it is cancelled, or the client is gone.
    fn hold(&mut self, session: &str) -> TurnEnd {
        self.record(json!({ "event": "holding", "session": session }));
        while let Some(message) = self.next_message() {
            if message["method"] == json!("session/cancel") {
                self.record(json!({ "event": "cancel", "session": session, "during": "a turn" }));
                return TurnEnd::Cancelled;
            }
            self.handle(message);
        }
        TurnEnd::Cancelled
    }

    // --- the MCP servers ---------------------------------------------------

    fn client(
        &mut self,
        session: &str,
        server: &str,
        known: &Value,
    ) -> Result<&mut McpClient, String> {
        let key = (session.to_string(), server.to_string());
        if !self.clients.contains_key(&key) {
            let spec = known["servers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|s| s["name"].as_str() == Some(server))
                .ok_or_else(|| format!("the session was handed no server {server}"))?;
            let log = self.beside.file(".mcp.log");
            let mut client = McpClient::start(spec, &log)?;
            // What it offers, asked once as a harness asks: the menu the
            // session really has, whatever a page says it should.
            let tools = match client.tools() {
                Ok(names) => json!(names),
                Err(e) => json!({ "error": e }),
            };
            self.record(json!({
                "event": "server_started",
                "session": session,
                "server": server,
                "answer": client.hello,
                "tools": tools,
            }));
            self.clients.insert(key.clone(), client);
        }
        self.clients
            .get_mut(&key)
            .ok_or_else(|| format!("no server {server}"))
    }

    /// Every tool the platform's server lists, each as a call with nothing
    /// in its hands. A server that cannot be asked is said, and nothing is
    /// called.
    fn every_tool(&mut self, session: &str, known: &Value) -> Vec<Value> {
        let listed = self
            .client(session, PLATFORM_SERVER, known)
            .and_then(McpClient::tools);
        match listed {
            Ok(names) => names.iter().map(|name| json!({ "name": name })).collect(),
            Err(e) => {
                self.record(json!({ "event": "tools_unlisted", "session": session, "error": e }));
                Vec::new()
            }
        }
    }

    fn close(&mut self) {
        for client in std::mem::take(&mut self.clients).into_values() {
            client.close();
        }
        self.record(json!({ "event": "ended", "why": "its input closed" }));
    }
}

enum Asked {
    Allowed,
    Refused,
    Cancelled,
}

/// Which of the platform's scopes a session serves, read off the arguments
/// its platform server was handed: a work item, a goal, a conversation.
struct Scope {
    kind: &'static str,
    args: Vec<String>,
}

impl Scope {
    fn of(servers: &Value) -> Self {
        let args: Vec<String> = servers
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["name"].as_str() == Some(PLATFORM_SERVER))
            .and_then(|s| s["args"].as_array())
            .map(|args| args.iter().map(text).collect())
            .unwrap_or_default();
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let kind = if has("--work-item") {
            "work_item"
        } else if has("--conversation") {
            "conversation"
        } else if has("--goal") {
            "goal"
        } else {
            "none"
        };
        Self { kind, args }
    }

    /// The value after `--<name>`, when the server was handed one.
    fn named(&self, name: &str) -> Option<String> {
        let flag = format!("--{name}");
        let at = self.args.iter().position(|a| *a == flag)?;
        self.args.get(at + 1).cloned()
    }
}

/// The arguments of a tool with what the session knows written in: `{{goal}}`,
/// `{{work_item}}`, `{{conversation}}`, `{{agent}}`, `{{cwd}}`, `{{prompt}}`,
/// `{{prompt.last}}` (its last line that says anything) — in a tool's
/// arguments and in what the agent says.
fn filled(arguments: &Value, scope: &Scope, known: &Value, prompt: &str) -> Value {
    let words: [(&str, String); 7] = [
        // The prompt's last line that says anything: the person's own words
        // at the foot of a conversation's transcript.
        (
            "{{prompt.last}}",
            prompt
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or_default()
                .to_string(),
        ),
        ("{{goal}}", scope.named("goal").unwrap_or_default()),
        (
            "{{work_item}}",
            scope.named("work-item").unwrap_or_default(),
        ),
        (
            "{{conversation}}",
            scope.named("conversation").unwrap_or_default(),
        ),
        ("{{agent}}", scope.named("agent").unwrap_or_default()),
        ("{{cwd}}", text(&known["cwd"])),
        ("{{prompt}}", prompt.to_string()),
    ];
    fn walk(value: &Value, words: &[(&str, String)]) -> Value {
        match value {
            Value::String(s) => {
                let mut out = s.clone();
                for (word, with) in words {
                    out = out.replace(word, with);
                }
                Value::String(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(|v| walk(v, words)).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), walk(v, words)))
                    .collect::<Map<String, Value>>(),
            ),
            other => other.clone(),
        }
    }
    match arguments {
        Value::Null => json!({}),
        given => walk(given, &words),
    }
}

/// A client of one MCP server over stdio: the handshake, then tools.
struct McpClient {
    child: Child,
    to: ChildStdin,
    from: BufReader<ChildStdout>,
    next: u64,
    /// What the server answered to `initialize`.
    hello: Value,
}

impl McpClient {
    /// Start the server as it was handed: the command, the arguments and
    /// the environment, its own words kept in `log`.
    fn start(spec: &Value, log: &Path) -> Result<Self, String> {
        let command = text(&spec["command"]);
        let mut process = Command::new(&command);
        process.args(spec["args"].as_array().into_iter().flatten().map(text));
        for pair in spec["env"].as_array().into_iter().flatten() {
            process.env(text(&pair["name"]), text(&pair["value"]));
        }
        let said = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .map_err(|e| format!("{}: {e}", log.display()))?;
        let mut child = process
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(said)
            .spawn()
            .map_err(|e| format!("{command} did not start: {e}"))?;
        let to = child.stdin.take().ok_or("the server has no input")?;
        let from = BufReader::new(child.stdout.take().ok_or("the server has no output")?);
        let mut client = Self {
            child,
            to,
            from,
            next: 0,
            hello: Value::Null,
        };
        client.hello = client.request(
            "initialize",
            json!({
                "protocolVersion": MCP_REVISION,
                "capabilities": {},
                "clientInfo": { "name": "scripted-agent", "version": "0" },
            }),
        )?;
        client.say(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        Ok(client)
    }

    fn say(&mut self, message: Value) -> Result<(), String> {
        writeln!(self.to, "{message}")
            .and_then(|()| self.to.flush())
            .map_err(|e| format!("the server stopped listening: {e}"))
    }

    /// One request and its answer; what the server says unasked is passed over.
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        self.say(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let mut line = String::new();
            let read = self
                .from
                .read_line(&mut line)
                .map_err(|e| format!("the server's answer could not be read: {e}"))?;
            if read == 0 {
                return Err(format!("the server ended before it answered {method}"));
            }
            let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            if message["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(error["message"].as_str().unwrap_or("refused").to_string());
            }
            return Ok(message["result"].clone());
        }
    }

    fn call(&mut self, name: &str, arguments: &Value) -> Result<Value, String> {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    /// The names of the tools the server lists, every page of them.
    fn tools(&mut self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        let mut cursor: Option<Value> = None;
        loop {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let page = self.request("tools/list", params)?;
            names.extend(
                page["tools"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|tool| text(&tool["name"])),
            );
            match page.get("nextCursor").filter(|next| !next.is_null()) {
                Some(next) => cursor = Some(next.clone()),
                None => return Ok(names),
            }
        }
    }

    /// The server's input closed: it ends by itself, and is waited for.
    fn close(self) {
        let Self { mut child, to, .. } = self;
        drop(to);
        let _ended = child.wait();
    }
}

/// A value's text: the string itself, or the value as JSON writes it.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The words of a prompt: its text blocks, in order.
fn prompt_text(prompt: &Value) -> String {
    prompt
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == json!("text"))
        .map(|block| text(&block["text"]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Said on stderr, which the daemon keeps with the session, and the end.
fn fail(why: &str) -> ! {
    eprintln!("scripted-agent: {why}");
    std::process::exit(2);
}
