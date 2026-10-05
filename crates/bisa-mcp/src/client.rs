//! Async JSONL client for the engine's intake unix socket.
//!
//! Wire protocol (must match `bisa-engine/src/intake.rs`): one JSON
//! object per line, LF-terminated; one reply line per request, in order.
//! Requests are scoped by EITHER `work_item` (worker sessions) or `goal`
//! (guided sessions) — see [`Scope`].

use bisa_core::{Answer, AskOption};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

/// What this MCP process is bound to for its lifetime: one work item
/// (worker session), one goal (guided session), one conversation
/// (a chat instance of an Agent — a DM or a channel it was mentioned in), or
/// one note (a session answering a question somebody asked about it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    WorkItem(String),
    /// One goal's cycle. `agent` is set when the guided session runs under
    /// an Agent definition; a human driving the cycle by hand has none.
    Goal {
        goal: String,
        agent: Option<String>,
    },
    /// The conversation and who is speaking in it. `goal` is the goal a
    /// goal thread, or a conversation about a goal, is about — set by the
    /// engine at launch; it is what lets the Workflow Agent shape the goal
    /// from its thread. A channel, a direct message or a conversation about
    /// anything else has none.
    Conversation {
        scope: String,
        agent: String,
        goal: Option<String>,
    },
}

impl Scope {
    /// The goal a session was launched knowing: a goal's cycle, or a
    /// conversation turn the engine launched with the goal it is about.
    pub fn goal(&self) -> Option<&str> {
        match self {
            Scope::Goal { goal, .. } => Some(goal),
            Scope::Conversation { goal, .. } => goal.as_deref(),
            Scope::WorkItem(_) => None,
        }
    }

    /// The agent this session speaks as, when it has one. A work item resolves
    /// its agent engine-side from the item itself, so it answers `None` here.
    pub fn agent(&self) -> Option<&str> {
        match self {
            Scope::WorkItem(_) => None,
            Scope::Goal { agent, .. } => agent.as_deref(),
            Scope::Conversation { agent, .. } => Some(agent),
        }
    }

    /// Test hook for the scope→request mapping (the wire contract the engine
    /// reads on the other end).
    #[doc(hidden)]
    pub fn apply_for_test(&self, req: &mut Value) {
        self.apply(req)
    }

    /// Insert this scope's fields into a request. Both agent-bearing scopes
    /// put `agent` on the wire, so every op resolves the signer and the recall
    /// owner from the request it already has rather than a second lookup.
    fn apply(&self, req: &mut Value) {
        match self {
            Scope::WorkItem(id) => req["work_item"] = json!(id),
            Scope::Goal { goal, agent } => {
                req["goal"] = json!(goal);
                // Omitted rather than sent as null when the cycle has no
                // agent: absent means "no signer", which the engine already
                // handles; a null would be a third case to special-case.
                if let Some(a) = agent {
                    req["agent"] = json!(a);
                }
            }
            Scope::Conversation { scope, agent, goal } => {
                // `scope` is only defaulted for ops that take one (post_message);
                // `agent` identifies the signer/recall owner.
                if req.get("scope").is_none() {
                    req["scope"] = json!(scope);
                }
                // The goal the engine launched this turn with rides every
                // request. Without one, the scope id goes out as the
                // candidate: a goal thread's scope id *is* the goal id — the
                // node posts into `goal.to_string()` — and a conversation
                // about a goal names it through its origin, so a session in
                // either can name the goal it is sitting in. A channel, DM
                // or other conversation id is not one, and the engine is the
                // discriminator: it looks the id up (`goal_of_scope_id`) and
                // answers "unknown goal", which is the correct answer there.
                if req.get("goal").is_none() {
                    req["goal"] = json!(goal.as_deref().unwrap_or(scope));
                }
                req["agent"] = json!(agent);
            }
        }
    }
}

/// Work that recurs, as the General Agent captures it: a statement that says
/// when — *every Monday…*, *whenever someone posts in #support…* — and a
/// title when it has one. The Workflow Agent designs its workflow with the
/// start event the statement names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandingGoal {
    pub statement: String,
    pub title: Option<String>,
}

/// What raising a named signal did: the record kept of it, and the signals
/// written for the starts that heard it — none when nothing listens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Raised {
    pub signal: String,
    pub listeners: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    #[error("intake socket unavailable at {path}: {source}")]
    Connect {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("intake connection broke mid-call: {0}")]
    Io(#[from] std::io::Error),
    #[error("intake replied with invalid JSON: {0}")]
    BadReply(String),
    #[error("intake closed the connection")]
    Closed,
}

/// Outcome of a `result_submit`: accepted, or rejected with validation errors
/// the model should see and retry on.
#[derive(Debug, Clone, PartialEq)]
pub enum SubmitOutcome {
    Accepted {
        result_event: String,
    },
    Rejected {
        errors: Vec<String>,
        attempts_left: u64,
    },
}

/// A resolved gate: the approve flag plus the free-text answer for text asks.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub approve: bool,
    pub answer: Option<Answer>,
    /// How many more clarification rounds this goal has, present only when
    /// the human said they were not sure. `Some(0)` means: stop asking,
    /// proceed on your own recommendation, journal the assumption.
    pub clarify_rounds_left: Option<u8>,
}

impl Decision {
    /// The human resolved the question without deciding it: they do not know.
    /// Never a decline — the caller must ask something narrower instead.
    pub fn is_unsure(&self) -> bool {
        self.answer.as_ref().map(|a| a.unsure).unwrap_or(false)
    }
}

/// Outcome of a base-hash-guarded write (recall): the new hash, or a
/// conflict carrying the current state so the model can merge.
#[derive(Debug, Clone, PartialEq)]
pub enum GuardedWrite {
    Written {
        hash: String,
    },
    Stale {
        errors: Vec<String>,
        current_hash: String,
        /// The current stored value, so the model can merge rather than
        /// re-read.
        current_value: Option<String>,
    },
}

type Framed = (
    BufReader<tokio::net::unix::OwnedReadHalf>,
    tokio::net::unix::OwnedWriteHalf,
);

/// One-request-one-reply JSONL client. Connection is lazy and cached;
/// a broken pipe triggers exactly one reconnect per call.
pub struct IntakeClient {
    path: PathBuf,
    conn: Mutex<Option<Framed>>,
}

/// `Ok` half of most calls: the reply value, or intake-reported errors the
/// model can act on.
pub type OpResult<T> = Result<T, Vec<String>>;

impl IntakeClient {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            conn: Mutex::new(None),
        }
    }

    pub fn socket_path(&self) -> &PathBuf {
        &self.path
    }

    async fn connect(&self) -> Result<Framed, IntakeError> {
        let stream =
            UnixStream::connect(&self.path)
                .await
                .map_err(|source| IntakeError::Connect {
                    path: self.path.clone(),
                    source,
                })?;
        let (r, w) = stream.into_split();
        Ok((BufReader::new(r), w))
    }

    /// Send one request line and read one reply line. Reconnects once if the
    /// cached connection turns out to be dead.
    async fn call(&self, request: &Value) -> Result<Value, IntakeError> {
        let mut line = request.to_string();
        line.push('\n');

        let mut guard = self.conn.lock().await;
        for attempt in 0..2 {
            if guard.is_none() {
                *guard = Some(self.connect().await?);
            }
            let conn = guard.as_mut().expect("connection just ensured");
            match Self::roundtrip(conn, line.as_bytes()).await {
                Ok(reply) => return Ok(reply),
                Err(e) => {
                    // Drop the dead connection; retry once with a fresh one.
                    *guard = None;
                    if attempt == 1 {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!("loop returns on second attempt")
    }

    async fn roundtrip(conn: &mut Framed, line: &[u8]) -> Result<Value, IntakeError> {
        let (reader, writer) = conn;
        writer.write_all(line).await?;
        writer.flush().await?;
        let mut reply = String::new();
        let n = reader.read_line(&mut reply).await?;
        if n == 0 {
            return Err(IntakeError::Closed);
        }
        serde_json::from_str(&reply).map_err(|e| IntakeError::BadReply(format!("{e}: {reply:?}")))
    }

    /// `ok:false` replies become `Err(errors)` in the outer `OpResult`.
    fn op_result(reply: Value) -> OpResult<Value> {
        if reply["ok"].as_bool() == Some(true) {
            Ok(reply)
        } else {
            Err(string_array(&reply["errors"]))
        }
    }

    // -- worker ops ---------------------------------------------------------

    pub async fn result_submit(
        &self,
        work_item: &str,
        output: Value,
    ) -> Result<SubmitOutcome, IntakeError> {
        let reply = self
            .call(&json!({"op": "result_submit", "work_item": work_item, "output": output}))
            .await?;
        if reply["ok"].as_bool() == Some(true) {
            Ok(SubmitOutcome::Accepted {
                result_event: reply["result_event"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            })
        } else {
            Ok(SubmitOutcome::Rejected {
                errors: string_array(&reply["errors"]),
                attempts_left: reply["attempts_left"].as_u64().unwrap_or(0),
            })
        }
    }

    pub async fn progress(
        &self,
        work_item: &str,
        verb: &str,
        object: &str,
        outcome: Option<&str>,
    ) -> Result<OpResult<()>, IntakeError> {
        let mut req =
            json!({"op": "progress", "work_item": work_item, "verb": verb, "object": object});
        if let Some(o) = outcome {
            req["outcome"] = json!(o);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|_| ()))
    }

    // -- questions ----------------------------------------------------------

    /// Opens an escalation gate; returns the gate id (non-blocking).
    /// `expects` is `"decision"` (default) or `"answer"`; `options` is the
    /// closed set of answers being offered, and may be empty.
    pub async fn ask_human(
        &self,
        scope: &Scope,
        question: &str,
        expects: Option<&str>,
        options: &[AskOption],
        multi: bool,
    ) -> Result<OpResult<String>, IntakeError> {
        let mut req = json!({"op": "ask_human", "question": question});
        scope.apply(&mut req);
        if let Some(e) = expects {
            req["expects"] = json!(e);
        }
        if !options.is_empty() {
            req["options"] = json!(options);
            req["multi"] = json!(multi);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["gate"].as_str().unwrap_or_default().to_string()))
    }

    /// Long-polls until the gate resolves; carries what the human said.
    pub async fn await_decision(&self, gate: &str) -> Result<OpResult<Decision>, IntakeError> {
        let reply = self
            .call(&json!({"op": "await_decision", "gate": gate}))
            .await?;
        Ok(Self::op_result(reply).map(|r| Decision {
            approve: r["approve"].as_bool().unwrap_or(false),
            answer: serde_json::from_value(r["answer"].clone()).unwrap_or(None),
            clarify_rounds_left: r["clarify_rounds_left"].as_u64().map(|n| n as u8),
        }))
    }

    // -- goal read/mutate -------------------------------------------------

    pub async fn get_goal(&self, scope: &Scope) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "get_goal"});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    /// The run a worker session works in — a goal's or the workspace's —
    /// which the engine reads off the work item: its steps, its own items,
    /// its home's journal tail, the ceiling it spends against and whose goal
    /// it is (`null` for a run of the workspace).
    pub async fn get_run(&self, scope: &Scope) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "get_run"});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    pub async fn revise_statement(
        &self,
        goal: &str,
        statement: &str,
        why: Option<&str>,
    ) -> Result<OpResult<()>, IntakeError> {
        let mut req = json!({"op": "revise_statement", "goal": goal, "statement": statement});
        if let Some(w) = why {
            req["why"] = json!(w);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|_| ()))
    }

    /// Propose the workflow a goal will run: a complete definition
    /// (`{name, description?, inputs?, steps, tags?}`). Returns the workflow
    /// id, its revision and the Adopt gate when one opened — the goal's mode
    /// decides. Only the Workflow Agent may call it, and the engine says so
    /// to anyone else.
    pub async fn propose_workflow(
        &self,
        goal: &str,
        agent: &str,
        workflow: Value,
    ) -> Result<OpResult<Proposed>, IntakeError> {
        let reply = self
            .call(
                &json!({"op": "propose_workflow", "goal": goal, "agent": agent,
                          "workflow": workflow}),
            )
            .await?;
        Ok(Self::op_result(reply).map(|r| Proposed {
            workflow: r["workflow"].as_str().unwrap_or_default().to_string(),
            revision: r["revision"].as_u64().unwrap_or(0),
            gate: r["gate"].as_str().map(str::to_string),
        }))
    }

    /// Propose an amendment to a goal's running workflow. Returns the gate
    /// the person decides — none when an auto goal applied it at once.
    pub async fn amend_workflow(
        &self,
        goal: &str,
        agent: &str,
        workflow: Value,
    ) -> Result<OpResult<Option<String>>, IntakeError> {
        let reply = self
            .call(
                &json!({"op": "amend_workflow", "goal": goal, "agent": agent,
                          "workflow": workflow}),
            )
            .await?;
        Ok(Self::op_result(reply).map(|r| r["gate"].as_str().map(str::to_string)))
    }

    pub async fn add_note(&self, scope: &Scope, text: &str) -> Result<OpResult<()>, IntakeError> {
        let mut req = json!({"op": "add_note", "text": text});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|_| ()))
    }

    /// Read one note. Returns `(title, body, hash)` — the hash is what a
    /// `note_write` states as the text it rewrites.
    pub async fn note_read(
        &self,
        scope: &Scope,
        note: Option<&str>,
    ) -> Result<OpResult<(String, String, String)>, IntakeError> {
        let mut req = json!({"op": "note_read"});
        if let Some(id) = note {
            req["note"] = json!(id);
        }
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| {
            (
                r["title"].as_str().unwrap_or_default().to_string(),
                r["body"].as_str().unwrap_or_default().to_string(),
                r["hash"].as_str().unwrap_or_default().to_string(),
            )
        }))
    }

    /// Rewrite a note's body at the hash read. Returns `(title, hash,
    /// changed)` — `changed` false when the text already read so and
    /// nothing was written.
    pub async fn note_write(
        &self,
        scope: &Scope,
        note: Option<&str>,
        text: &str,
        base_hash: &str,
    ) -> Result<OpResult<(String, String, bool)>, IntakeError> {
        let mut req = json!({"op": "note_write", "text": text, "base_hash": base_hash});
        if let Some(id) = note {
            req["note"] = json!(id);
        }
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| {
            (
                r["title"].as_str().unwrap_or_default().to_string(),
                r["hash"].as_str().unwrap_or_default().to_string(),
                r["changed"].as_bool().unwrap_or(true),
            )
        }))
    }

    /// Review notes on the projects this session can see. The
    /// reply is the engine's JSON: `projects` and `notes`.
    pub async fn review_notes_list(
        &self,
        scope: &Scope,
        project: Option<&str>,
        include_resolved: bool,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "review_notes_list", "include_resolved": include_resolved});
        if let Some(p) = project {
            req["project"] = json!(p);
        }
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    /// Mark a review note dealt with. Returns the note as it is now.
    pub async fn review_note_resolve(
        &self,
        scope: &Scope,
        note: &str,
        project: Option<&str>,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "review_note_resolve", "note": note});
        if let Some(p) = project {
            req["project"] = json!(p);
        }
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["note"].clone()))
    }

    /// The GitHub reviews and resolvable threads on the PR of the workstream
    /// this session runs in.
    /// Ask the embedded browser to do something (ide/18); the engine waits for
    /// the desktop and answers `result` — `ok: false` with `error` when no
    /// desktop is there.
    pub async fn browser(
        &self,
        scope: &Scope,
        request: Value,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "browser", "request": request});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["result"].clone()))
    }

    /// One drawing request (19 — Drawings); the engine answers `result` —
    /// `ok: false` with `error` when it refuses or no desktop is there.
    pub async fn draw(
        &self,
        scope: &Scope,
        request: Value,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "draw", "request": request});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["result"].clone()))
    }

    /// `decide`: typed questions about a state, put to the Decision-Making
    /// Agent. Answers `{sure, response}`; refused in a sentence when the
    /// Decision-Making Agent is not on for the session's agent.
    pub async fn decide(
        &self,
        scope: &Scope,
        request: Value,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "decide", "request": request});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["result"].clone()))
    }

    /// `browser_serve` (ide/18): the platform serves a folder of the
    /// session's checkout and answers the URL to open.
    pub async fn browser_serve(
        &self,
        scope: &Scope,
        folder: Option<String>,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "browser_serve", "folder": folder});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["result"].clone()))
    }

    /// One mobile request (ide/19): the node asks this machine's simulators
    /// and `adb` itself and answers the facts; a refusal is the op's error.
    pub async fn mobile_development(
        &self,
        scope: &Scope,
        request: Value,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "mobile_development", "request": request});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["result"].clone()))
    }

    pub async fn pr_reviews_list(&self, scope: &Scope) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "pr_reviews_list"});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    /// Submit a GitHub review on that workstream's PR. `event` is
    /// `approve` · `request_changes` · `comment`; `comments` is an array of
    /// `{path, line, body}`.
    pub async fn pr_review_submit(
        &self,
        scope: &Scope,
        event: &str,
        body: &str,
        comments: Value,
    ) -> Result<OpResult<()>, IntakeError> {
        // No words is no body — never `""`, which a code host reads as a
        // wordless comment and refuses.
        let body = (!body.trim().is_empty()).then(|| body.trim());
        let mut req =
            json!({"op": "pr_review_submit", "event": event, "body": body, "comments": comments});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|_| ()))
    }

    /// Reply on one review thread of that workstream's PR, and resolve it in
    /// the same act when `resolve`. The engine signs the reply with the
    /// session's agent.
    pub async fn pr_thread_reply(
        &self,
        scope: &Scope,
        thread: &str,
        body: &str,
        resolve: bool,
    ) -> Result<OpResult<()>, IntakeError> {
        let mut req = json!({"op": "pr_thread_reply", "thread": thread, "body": body.trim(), "resolve": resolve});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|_| ()))
    }

    /// Resolve (or reopen) one review thread of that workstream's PR.
    pub async fn pr_thread_resolve(
        &self,
        scope: &Scope,
        thread: &str,
        resolved: bool,
    ) -> Result<OpResult<()>, IntakeError> {
        let mut req = json!({"op": "pr_thread_resolve", "thread": thread, "resolved": resolved});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|_| ()))
    }

    /// Append a block to a note. Returns the note's title.
    pub async fn note_append(
        &self,
        scope: &Scope,
        note: Option<&str>,
        text: &str,
    ) -> Result<OpResult<String>, IntakeError> {
        let mut req = json!({"op": "note_append", "text": text});
        if let Some(id) = note {
            req["note"] = json!(id);
        }
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["title"].as_str().unwrap_or_default().to_string()))
    }

    pub async fn spawn_sub_goal(
        &self,
        scope: &Scope,
        statement: &str,
        title: Option<&str>,
    ) -> Result<OpResult<String>, IntakeError> {
        let mut req = json!({"op": "spawn_sub_goal", "statement": statement});
        scope.apply(&mut req);
        if let Some(t) = title {
            req["title"] = json!(t);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["child"].as_str().unwrap_or_default().to_string()))
    }

    /// Raise a named signal.
    ///
    /// Calls the engine's `emit_signal` op (`bisa-engine/src/intake.rs`),
    /// which goes through the one emit door — the same an `emit` step, the
    /// node's `POST /signals` and `bisa signal emit` take — so the name
    /// rule, the causal chain and the payload cap apply here too. The signal
    /// is heard by every `signal` start, wait and boundary event that names
    /// it.
    pub async fn emit_signal(
        &self,
        scope: &Scope,
        name: &str,
        payload: Value,
        signal_scope: Option<&str>,
    ) -> Result<OpResult<Raised>, IntakeError> {
        let mut req = json!({"op": "emit_signal", "name": name, "payload": payload});
        scope.apply(&mut req);
        if let Some(s) = signal_scope {
            // The signal's own scope, distinct from the session's.
            req["signal_scope"] = json!(s);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| Raised {
            signal: r["signal"].as_str().unwrap_or_default().to_string(),
            listeners: string_array(&r["listeners"]),
        }))
    }

    // -- conversation / recall ---------------------------------------------------

    /// `scope_ulid` is the channel/goal/workstream the message belongs to.
    /// Who speaks rides with it — the session's agent, else its work item —
    /// so the engine signs the post as that agent and lets it publish from
    /// where it works; a post with no speaker is signed as the General
    /// Agent and may carry no file.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_message(
        &self,
        agent: Option<&str>,
        work_item: Option<&str>,
        scope_ulid: &str,
        content: &str,
        reply_to: Option<&str>,
        mentions: &[String],
        attachments: &[String],
        artifacts: &[Value],
    ) -> Result<OpResult<String>, IntakeError> {
        let mut req = json!({"op": "post_message", "scope": scope_ulid, "content": content});
        Self::identity(&mut req, agent, work_item);
        if let Some(r) = reply_to {
            req["reply_to"] = json!(r);
        }
        // Omitted when empty rather than sent as `[]`: the engine's default is
        // already "addresses nobody", and an absent field cannot be confused
        // with a caller that meant to clear one.
        if !mentions.is_empty() {
            req["mentions"] = json!(mentions);
        }
        if !attachments.is_empty() {
            req["attachments"] = json!(attachments);
        }
        if !artifacts.is_empty() {
            req["artifacts"] = json!(artifacts);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["message"].as_str().unwrap_or_default().to_string()))
    }

    /// Who a session speaks and remembers as: its agent, else the work item
    /// the engine resolves the agent from.
    fn identity(req: &mut Value, agent: Option<&str>, work_item: Option<&str>) {
        if let Some(a) = agent {
            req["agent"] = json!(a);
        } else if let Some(wi) = work_item {
            req["work_item"] = json!(wi);
        }
    }

    pub async fn recall_store(
        &self,
        agent: Option<&str>,
        work_item: Option<&str>,
        slug: &str,
        value: &str,
        base_hash: Option<&str>,
    ) -> Result<GuardedWrite, IntakeError> {
        let mut req = json!({"op": "recall_store", "slug": slug, "value": value});
        Self::identity(&mut req, agent, work_item);
        if let Some(h) = base_hash {
            req["base_hash"] = json!(h);
        }
        let reply = self.call(&req).await?;
        if reply["ok"].as_bool() == Some(true) {
            Ok(GuardedWrite::Written {
                hash: reply["hash"].as_str().unwrap_or_default().to_string(),
            })
        } else {
            Ok(GuardedWrite::Stale {
                errors: string_array(&reply["errors"]),
                current_hash: reply["current_hash"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                current_value: reply["current_value"].as_str().map(str::to_string),
            })
        }
    }

    /// Returns `Some((value, hash, links))` when the slug exists.
    #[allow(clippy::type_complexity)]
    pub async fn recall_get(
        &self,
        agent: Option<&str>,
        work_item: Option<&str>,
        slug: &str,
    ) -> Result<OpResult<Option<(String, String, Vec<String>)>>, IntakeError> {
        let mut req = json!({"op": "recall_get", "slug": slug});
        Self::identity(&mut req, agent, work_item);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| {
            r["value"].as_str().map(|v| {
                (
                    v.to_string(),
                    r["hash"].as_str().unwrap_or_default().to_string(),
                    string_array(&r["links"]),
                )
            })
        }))
    }

    pub async fn recall_list(
        &self,
        agent: Option<&str>,
        work_item: Option<&str>,
    ) -> Result<OpResult<Vec<Value>>, IntakeError> {
        let mut req = json!({"op": "recall_list"});
        Self::identity(&mut req, agent, work_item);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["records"].as_array().cloned().unwrap_or_default()))
    }

    // -- platform ----------------------------------------------------------
    //
    // Workspace-wide ops. Every one of them names the calling `agent`
    // explicitly instead of relying on the scope: the engine authorises them
    // against the core agent id, and an op whose authorisation depends on
    // which scope variant happened to fill the field in is an op whose
    // authorisation is hard to read.

    /// The whole workspace in one reply: staff, structure, work in flight and
    /// what the catalog still holds.
    pub async fn workspace_overview(&self, agent: &str) -> Result<OpResult<Value>, IntakeError> {
        let reply = self
            .call(&json!({"op": "workspace_overview", "agent": agent}))
            .await?;
        Ok(Self::op_result(reply))
    }

    /// Who may be named on a step: the enabled agents and teams, with what each
    /// does and who is on each team — rendered by the engine as `text`.
    /// Who may be named on a step — for `goal`, that goal's roster: the
    /// agents and teams it names to carry it, when it names any. `goal` rides
    /// the wire only when there is one.
    pub async fn list_staff(
        &self,
        agent: &str,
        goal: Option<&str>,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "list_staff", "agent": agent});
        if let Some(goal) = goal {
            req["goal"] = json!(goal);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    /// Catalog entries, optionally one kind only. `installed` on each entry is
    /// what lets a caller propose from what exists rather than invent a name.
    pub async fn list_catalog(
        &self,
        agent: &str,
        kind: Option<&str>,
    ) -> Result<OpResult<Vec<Value>>, IntakeError> {
        let reply = self
            .call(&json!({"op": "list_catalog", "agent": agent, "kind": kind}))
            .await?;
        Ok(Self::op_result(reply).map(|r| r["entries"].as_array().cloned().unwrap_or_default()))
    }

    /// Install one entry. The reply's `installed` object names everything the
    /// install created, not just the entry asked for — installs are transitive
    /// (a team brings its agents, an agent its skills, a channel its roster).
    pub async fn install_catalog_entry(
        &self,
        agent: &str,
        kind: &str,
        slug: &str,
        goal: Option<&str>,
    ) -> Result<OpResult<Value>, IntakeError> {
        let reply = self
            .call(&json!({"op": "install_catalog_entry", "agent": agent,
                          "kind": kind, "slug": slug, "goal": goal}))
            .await?;
        Ok(Self::op_result(reply).map(|r| r["installed"].clone()))
    }

    /// Put assignees (`agent:<id>` / `team:<id>` / `human:<64 hex>`) on an
    /// goal, or on one of its projects. Returns the resulting full list, so
    /// the caller reports what is true afterwards rather than what it sent.
    pub async fn assign(
        &self,
        agent: &str,
        goal: &str,
        project: Option<&str>,
        assignees: &[String],
        replace: bool,
    ) -> Result<OpResult<Vec<String>>, IntakeError> {
        let reply = self
            .call(&json!({"op": "assign", "agent": agent, "goal": goal,
                          "project": project, "assignees": assignees, "replace": replace}))
            .await?;
        Ok(Self::op_result(reply).map(|r| string_array(&r["assignees"])))
    }

    /// Capture a standing goal. Returns its id; capturing starts nothing —
    /// the Workflow Agent designs the goal's workflow, and the goal listens
    /// once that design is adopted.
    pub async fn capture_goal(
        &self,
        agent: &str,
        goal: &StandingGoal,
    ) -> Result<OpResult<String>, IntakeError> {
        let reply = self
            .call(&json!({"op": "capture_goal", "agent": agent,
                          "statement": goal.statement, "title": goal.title}))
            .await?;
        Ok(Self::op_result(reply).map(|r| r["goal"].as_str().unwrap_or_default().to_string()))
    }

    /// Call one read operation of a connector, as the session: the screened
    /// answer, or the sentence the screen held it behind.
    pub async fn call_connector(
        &self,
        scope: &Scope,
        connector: &str,
        operation: &str,
        account: Option<&str>,
        params: &Value,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({
            "op": "call_connector",
            "connector": connector,
            "operation": operation,
            "params": params,
        });
        if let Some(a) = account {
            req["account"] = json!(a);
        }
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    /// The connectors installed here with their operations and accounts:
    /// `{connectors: [...], text: "CONNECTORS — ..."}`.
    ///
    /// Every session's to read, so who asks is named the way recall names
    /// it — the scope's agent, or the work item the engine resolves one
    /// from — and never required.
    pub async fn list_connectors(
        &self,
        agent: Option<&str>,
        work_item: Option<&str>,
    ) -> Result<OpResult<Value>, IntakeError> {
        let mut req = json!({"op": "list_connectors"});
        Self::identity(&mut req, agent, work_item);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply))
    }

    /// The catalog's workflow templates and this workspace's own workflows:
    /// `{templates: [catalog entries], workflows: [{id, name, …}]}`.
    pub async fn list_workflow_templates(
        &self,
        agent: &str,
    ) -> Result<OpResult<Value>, IntakeError> {
        let reply = self
            .call(&json!({"op": "list_workflow_templates", "agent": agent}))
            .await?;
        Ok(Self::op_result(reply))
    }

    /// One workflow by id or catalog slug, with its problems:
    /// `{workflow, installed, problems, note?}`.
    pub async fn get_workflow(
        &self,
        agent: &str,
        workflow: &str,
    ) -> Result<OpResult<Value>, IntakeError> {
        let reply = self
            .call(&json!({"op": "get_workflow", "agent": agent, "workflow": workflow}))
            .await?;
        Ok(Self::op_result(reply))
    }

    /// Write the library workflow the session's conversation is about, at
    /// `revision`: `{workflow, revision}` of the copy now stored. The scope
    /// rides with the request ([`Scope::apply`] — the conversation's id and
    /// the agent), and the engine names the workflow from it.
    pub async fn save_workflow(
        &self,
        scope: &Scope,
        revision: u64,
        definition: Value,
    ) -> Result<OpResult<Saved>, IntakeError> {
        let mut req =
            json!({"op": "save_workflow", "revision": revision, "definition": definition});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| Saved {
            workflow: r["workflow"].as_str().unwrap_or_default().to_string(),
            revision: r["revision"].as_u64().unwrap_or(0),
        }))
    }

    /// Every problem a definition has, without recording it — its staffing
    /// judged against `goal`'s roster when there is one.
    pub async fn validate_workflow(
        &self,
        agent: &str,
        workflow: Value,
        goal: Option<&str>,
    ) -> Result<OpResult<Vec<Value>>, IntakeError> {
        let mut req = json!({"op": "validate_workflow", "agent": agent, "workflow": workflow});
        if let Some(goal) = goal {
            req["goal"] = json!(goal);
        }
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| r["problems"].as_array().cloned().unwrap_or_default()))
    }

    /// Create a managed project under a goal. Returns `(id, slug, path)`.
    ///
    /// Scoped like every other op rather than taking an agent argument: every
    /// session may create a project, and a work-item session has no agent on
    /// its scope at all. What `apply` puts on the wire — `agent` here, a
    /// `work_item` there — is what the engine signs the journal note with.
    /// `goal` is already set, so the conversation arm leaves it alone.
    pub async fn create_project(
        &self,
        scope: &Scope,
        p: &NewProjectRequest,
    ) -> Result<OpResult<(String, String, String)>, IntakeError> {
        let mut req = json!({"op": "create_project", "goal": p.goal,
                             "slug": p.slug, "name": p.name,
                             "assignees": p.assignees});
        scope.apply(&mut req);
        let reply = self.call(&req).await?;
        Ok(Self::op_result(reply).map(|r| {
            (
                r["project"].as_str().unwrap_or_default().to_string(),
                r["slug"].as_str().unwrap_or_default().to_string(),
                r["path"].as_str().unwrap_or_default().to_string(),
            )
        }))
    }
}

/// What `propose_workflow` recorded: the workflow, its revision and the gate
/// the person adopts it through.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposed {
    pub workflow: String,
    pub revision: u64,
    /// The Adopt gate that opened for the person — none when the platform
    /// adopted it alone (an auto goal) or recorded it as the person's draft
    /// (a manual goal).
    pub gate: Option<String>,
}

/// A library workflow the Workflow Agent saved from its conversation: the id
/// and the revision now stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub workflow: String,
    pub revision: u64,
}

/// One project to create. There is no `git` field: the op only ever makes a
/// managed folder under a goal, and a folder we made is one we initialise.
#[derive(Debug, Clone)]
pub struct NewProjectRequest {
    /// The goal to attach the new project to, when the session has one. A
    /// project belongs to the workspace either way.
    pub goal: Option<String>,
    pub slug: String,
    pub name: Option<String>,
    pub assignees: Vec<String>,
}

fn string_array(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|e| e.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}
