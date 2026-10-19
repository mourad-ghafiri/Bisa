//! The Muse Session Protocol — JSON-RPC 2.0 on stdio, one message a line —
//! and the door every Muse Code session goes through.
//!
//! **The protocol is this module; a harness is its facts.** Muse Code
//! (`muse_code.rs`) is the one harness that speaks it, and no frame of it is
//! written outside this file. Read on 2026-10-19 from the SDK's wire guide
//! and reference (<https://meta-models.github.io/muse-code-sdk/1.4/>), its
//! schema and the recorded conformance transcripts under `schema/msp/` of
//! <https://github.com/meta-models/muse-code-sdk>, and the manual
//! (<https://dev.meta.ai/docs/muse-code>):
//!
//! - **The host.** `muse serve` takes one client over its stdin and stdout —
//!   NDJSON, every message a JSON-RPC 2.0 object on one line — and holds
//!   zero or more sessions, each owned by one host at a time through a lease
//!   on disk. A client sends `initialize`, reads the result, sends the
//!   `initialized` notification; until then every other request is refused.
//!   The result names the host (`serverInfo`), where it keeps its files
//!   (`museHome`), whether its sessions persist (`sessionDurability`) and
//!   the schema it speaks (`schema.version`, `schema.fingerprint`) — a
//!   fingerprint that moved is said in the log and never refused, the SDK's
//!   own rule for a protocol that is pre-1.0.
//! - **Commands.** Every call that changes state carries a client-minted
//!   `commandId`, a UUIDv7, so a retry joins the original: `session/start`,
//!   `session/resume`, `session/setModel`, `turn/start`, `turn/cancel`,
//!   `approval/decide`, `userInput/answer`. `model/list` and `usage/read`
//!   are queries.
//! - **The session.** `session/start` names the workspace root (absolute),
//!   the model, the approval mode and the MCP servers of this session alone
//!   (`config.mcpServers`); the host says `session/started`, then answers
//!   with the session and a view cursor. `session/resume` reloads one by its
//!   id, metadata only (`excludeItems: true`), and the host re-issues every
//!   request the session was parked on. The model is checked against the
//!   host's own `model/list` **before** the session is started: a model the
//!   sign-in does not list ends the launch as *model unavailable*, typed,
//!   before any session file is written, so the plan walks on.
//! - **The turn.** `turn/start` carries the input parts and, this turn only,
//!   the reasoning effort; its ack says whether the turn started, queued or
//!   steered (`ifBusy`). The host streams items — `item/started`,
//!   `item/delta` (a message's `text`, a reasoning's `summary.n`),
//!   `item/completed` — a `toolCall`, a `subagent`, a `webSearch` among
//!   them, and ends the turn with `turn/completed`: `completed`, `cancelled`,
//!   or `failed` with a typed error.
//! - **Approvals.** Under `promptUnmatched` the host asks before a shell
//!   stage or a tool its policy does not already answer: a notification
//!   (`approval/requested`) and a request (`approval/request`) with the same
//!   params. The request is answered `{}` at once — *I am handling this* —
//!   and the decision travels as the `approval/decide` command, selecting a
//!   choice the host offered: the one whose decision is *approved* for an
//!   allow, never *approvedForSession* or an amendment; *denied*, else
//!   *abort*, for a deny. Approvals never expire: a parked one holds its
//!   turn and survives a restart.
//! - **The end.** Stdin closed, the host drains, writes its session-end
//!   records and exits 0; `SIGTERM` is 143. Exit 2 is bad words or a
//!   version mismatch, 3 no configuration or sign-in, 4 a session held by
//!   another host, 5 the session protocol switched off; stderr is captured
//!   and never parsed.
//!
//! Implemented as raw JSON-RPC rather than through the SDK: the adapter
//! needs a thin, version-tolerant subset, and owning the frames keeps the
//! mapper in one place with no API churn.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bisa_core::caps::canonical_tool;
use bisa_core::ToolTier;
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    Attachment, BoxEventStream, Effort, HarnessError, HarnessSession, InputAnswer, InputRequest,
    LifecycleEvent, McpMount, McpServerConfig, ModelInfo, Outcome, Phase, ProgressEvent,
    PromptInput, ResumeToken, SessionEvent, SessionSnapshot, SessionSpec, Steer, SubagentId,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::util::{self, Drive, ModelCtx, OutMsg, Shared};
use bisa_core::sync::Locked;

/// The envelope schema version the host is expected to speak
/// (`schema/msp/stable/manifest.json`, read 2026-10-19).
pub const SCHEMA_VERSION: u64 = 1;

/// The stable surface's fingerprint this module was written against (the
/// same manifest). A host that answers another is said in the log, never
/// refused.
pub const SCHEMA_FINGERPRINT: &str =
    "sha256:5d3b43e5ccdb7dc4f5a32c380f5f2cde0f10e3059b93610afb2e73f73b1d914c";

/// The approval mode every session is started in: the strictest that still
/// asks. `denyUnmatched` never asks and `onRequest` lets the model decide;
/// `allowAll` is never sent.
pub const APPROVAL_MODE: &str = "promptUnmatched";

/// The protocol's reasoning efforts the platform has words for, lowest
/// first. The host also knows `none` and `ultra`; neither is ever sent, and
/// a model's row that lists them is read without them.
pub const LEVELS: [Effort; 6] = Effort::ALL;

/// Request ids used by the driver's handshake.
const ID_INIT: u64 = 1;
const ID_MODELS: u64 = 2;
const ID_SESSION: u64 = 3;
const ID_SET_MODEL: u64 = 4;
const ID_FIRST_TURN: u64 = 5;
/// The facade's ids begin here.
const FACADE_IDS_FROM: u64 = 100;

/// How many items a session is remembered to have open at once; a turn that
/// ends forgets them all.
const MAX_KNOWN_ITEMS: usize = 256;

/// How much of a tool call's arguments a progress event carries.
const ARGS_SUMMARY: usize = 200;

/// Why a session on a model the host does not list ends.
const MODEL_NOT_LISTED: &str = "the host does not list this model for this sign-in";

/// How one host is started: whose session it is, the program, and the words
/// that make it a host. Everything else a session needs — where it runs, its
/// environment, its MCP servers, the model and the effort — is the
/// [`SessionSpec`]'s, and reaches the host the protocol's way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MspCommand {
    /// The adapter the session belongs to.
    pub adapter_id: String,
    pub program: String,
    pub args: Vec<String>,
}

impl MspCommand {
    pub fn new(
        adapter_id: impl Into<String>,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

/// One row of `model/list`, as the adapter keeps it between calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub id: String,
    pub label: Option<String>,
    pub is_default: bool,
    /// The efforts the row lists that the platform has words for, lowest
    /// first; empty when the host says nothing of them.
    pub variants: Vec<Effort>,
}

/// The rows `model/list` answered: `models[]`, each a `modelId`, a
/// `displayLabel`, whether it is the catalog's default and its `variants` —
/// the default first, each id once.
pub fn rows_of(result: &Value) -> Vec<ModelRow> {
    let rows: Vec<ModelRow> = result
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let id = row.get("modelId").and_then(Value::as_str)?;
            let mut variants: Vec<Effort> = row
                .get("variants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|word| word.parse().ok())
                .collect();
            variants.sort();
            variants.dedup();
            Some(ModelRow {
                id: id.to_string(),
                label: row
                    .get("displayLabel")
                    .and_then(Value::as_str)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string),
                is_default: row.get("isDefault").and_then(Value::as_bool) == Some(true),
                variants,
            })
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    rows.iter()
        .filter(|r| r.is_default)
        .chain(rows.iter().filter(|r| !r.is_default))
        .filter(|r| seen.insert(r.id.clone()))
        .cloned()
        .collect()
}

/// The rows as the catalog lists models: each with the levels it takes — its
/// own variants, or every level when the host said nothing of them.
pub fn listed(rows: &[ModelRow]) -> Vec<ModelInfo> {
    rows.iter()
        .map(|r| ModelInfo::new(&r.id, r.label.as_deref(), levels_of_row(r)))
        .collect()
}

fn levels_of_row(row: &ModelRow) -> Vec<Effort> {
    if row.variants.is_empty() {
        LEVELS.to_vec()
    } else {
        row.variants.clone()
    }
}

/// The levels a model takes: its row's variants — the default row's for no
/// model — and every level for a model nobody lists.
pub fn levels_of(rows: &[ModelRow], model: Option<&str>) -> Vec<Effort> {
    let row = match model {
        Some(id) => rows.iter().find(|r| r.id == id),
        None => rows.iter().find(|r| r.is_default).or(rows.first()),
    };
    row.map(levels_of_row).unwrap_or_else(|| LEVELS.to_vec())
}

/// The word a turn carries for `asked`, held to the levels the model takes
/// ([`levels_of`]): itself, else the nearest below, else the lowest above.
pub fn effort_word(rows: &[ModelRow], model: Option<&str>, asked: Effort) -> Option<String> {
    asked
        .clamp_to(&levels_of(rows, model))
        .map(|level| level.as_str().to_string())
}

/// A UUIDv7 for a command: the time in its first 48 bits, the version and
/// the variant where the standard puts them, the rest random — minted here,
/// since the host never mints one and a retry must carry the same id.
pub fn command_id() -> String {
    let mut bytes = [0u8; 16];
    if let Err(e) = getrandom::fill(&mut bytes) {
        // The system's random source refusing is not a reason to stop a
        // session: the time and the counter still make the id its own.
        tracing::warn!(target: "bisa_adapters::msp", "random bytes for a command id: {e}"); // LCOV_EXCL_LINE: the OS refusing random bytes; nothing on a test machine says why
        let n = NONCE.fetch_add(1, Ordering::Relaxed).to_be_bytes(); // LCOV_EXCL_LINE: the counter that stands in when the OS refuses random bytes, which no test machine does
        bytes[8..16].copy_from_slice(&n); // LCOV_EXCL_LINE: the counter written in place of the random tail, reached only under that refusal
    }
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

static NONCE: AtomicU64 = AtomicU64::new(1);

fn jsonrpc_request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn jsonrpc_notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

fn jsonrpc_response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// The session's MCP servers in the protocol's own shape
/// (`SessionMcpServerConfig`): an object by name — a stdio server is its
/// `command`, `args` and `env` as an object; a remote one `streamableHttp`
/// with its `url` and `headers`. The protocol has no `cwd` for a stdio
/// server and no SSE transport: the first is dropped with a warning, the
/// second whole.
fn mcp_servers_json(adapter_id: &str, mounts: &[McpMount]) -> Value {
    let mut servers = serde_json::Map::new();
    for mount in mounts {
        match &mount.config {
            McpServerConfig::Stdio {
                name,
                command,
                args,
                env,
                cwd,
            } => {
                if cwd.is_some() {
                    crate::mcp_inject::warn_dropped(adapter_id, name, "cwd");
                }
                servers.insert(
                    name.clone(),
                    json!({ "transport": "stdio", "command": command, "args": args, "env": env }),
                );
            }
            McpServerConfig::Http { name, url, headers } => {
                servers.insert(
                    name.clone(),
                    json!({ "transport": "streamableHttp", "url": url, "headers": headers }),
                );
            }
            McpServerConfig::Sse { name, .. } => {
                crate::mcp_inject::warn_dropped(adapter_id, name, "sse transport");
            }
        }
    }
    Value::Object(servers)
}

/// A tool call's arguments as the host carries them — a JSON string that
/// parses is that JSON; anything else is itself.
fn args_value(raw: &Value) -> Value {
    match raw.as_str() {
        Some(text) => serde_json::from_str(text).unwrap_or_else(|_| raw.clone()),
        None => raw.clone(),
    }
}

/// The tier of a tool by its neutral name; an unknown one is the most it
/// could be.
fn tier_of(tool: &str) -> ToolTier {
    ToolTier::classify(canonical_tool(tool).unwrap_or(tool))
}

/// The choice that answers an approval: for an allow the one whose decision
/// is *approved* — once, never for the session nor an amendment the guard
/// did not give; for a deny the one whose decision is *denied*, else
/// *abort*. `None` when the host offered nothing of the kind.
fn choice_for(choices: &[Value], allow: bool) -> Option<Value> {
    fn decision_of(choice: &Value) -> &str {
        choice.get("decision").and_then(Value::as_str).unwrap_or("")
    }
    let wanted: &[&str] = if allow {
        &["approved"]
    } else {
        &["denied", "abort"]
    };
    wanted
        .iter()
        .find_map(|want| choices.iter().find(|c| decision_of(c) == *want))
        .cloned()
}

/// How a turn ended, from `turn/completed`: completed, cancelled, or failed
/// — a model's wall typed by the host (a quota, a retryable model error)
/// walks the plan; a missing sign-in and the rest are failures in words.
fn turn_outcome(ctx: &ModelCtx, completed: &Value) -> Outcome {
    match completed.get("terminal").and_then(Value::as_str) {
        Some("completed") => Outcome::Completed,
        Some("cancelled") => Outcome::Aborted,
        _ => {
            let error = completed.get("error").unwrap_or(&Value::Null);
            let kind = error
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("failed");
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| completed.get("reason").and_then(Value::as_str))
                .unwrap_or("the turn failed");
            let retryable = error.get("retryable").and_then(Value::as_bool) == Some(true);
            let quota = error.get("quota").filter(|q| q.is_object());
            if kind == "authRequired" {
                return Outcome::Failed {
                    error:
                        "Muse Code needs a sign-in here: run `muse` and sign in, or `muse auth set`"
                            .to_string(),
                };
            }
            if quota.is_some() || (kind == "modelError" && retryable) {
                let retry_after = quota
                    .and_then(|q| {
                        q.get("resetsAtMs")
                            .or_else(|| q.get("retryAfterMs"))
                            .and_then(Value::as_u64)
                    })
                    .map(|ms| ms / 1_000);
                return ctx.typed_outcome(format!("{kind}: {message}"), retry_after);
            }
            ctx.outcome(message).unwrap_or(Outcome::Failed {
                error: format!("{kind}: {message}"),
            })
        }
    }
}

/// What a host's exit code says, for the ones the manual names.
fn exit_words(code: i32) -> Option<&'static str> {
    Some(match code {
        2 => "muse serve refused its arguments or the protocol version (exit 2)",
        3 => "Muse Code is not signed in or not configured here (exit 3): run `muse` and sign in, or `muse auth set`",
        4 => "the session is held by another muse host (exit 4): close it and attach again",
        5 => "Muse Code's session protocol is off here (MUSE_EXPERIMENTAL_SDK_ENABLED=off, exit 5)",
        _ => return None,
    })
}

/// What a host that left has done, by its exit code and whether its session
/// existed: finished, once a session did; failed to start when it left
/// before; the manual's codes in words; a signal as an interruption; and
/// anything else classified by its stderr tail.
pub fn exit_outcome(
    code: Option<i32>,
    existed: bool,
    stderr_tail: &[String],
    ctx: &ModelCtx,
) -> Outcome {
    let tail = stderr_tail.join(" | ");
    let with_tail = |words: &str| {
        if tail.is_empty() {
            words.to_string()
        } else {
            format!("{words}; stderr tail: {tail}")
        }
    };
    match code {
        Some(0) if existed => Outcome::Completed,
        Some(0) => Outcome::Failed {
            error: with_tail("the host ended before its session began"),
        },
        Some(code) if exit_words(code).is_some() => Outcome::Failed {
            error: exit_words(code).unwrap_or("").to_string(),
        },
        Some(130) | Some(143) => Outcome::Failed {
            error: with_tail(&format!(
                "the host was interrupted (exit {})",
                code.unwrap_or(0)
            )),
        },
        other => ctx.outcome(&tail).unwrap_or_else(|| Outcome::Failed {
            error: match other {
                Some(code) => with_tail(&format!("process exited with {code}")),
                None => with_tail("process ended by a signal"),
            },
        }),
    }
}

/// One image attachment as an input part: its bytes, base64, and its type.
/// `None` for a file that cannot be read — it stays named in the text.
fn image_part(attachment: &Attachment) -> Option<Value> {
    use base64::Engine;
    let bytes = match std::fs::read(&attachment.path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(target: "bisa_adapters::msp", path = %attachment.path.display(), "an image attachment could not be read and stays a path in the text: {e}");
            return None;
        }
    };
    Some(json!({
        "type": "image",
        "base64Data": base64::engine::general_purpose::STANDARD.encode(bytes),
        "mediaType": attachment.mime,
    }))
}

/// The `turn/start` request: the text, every image the message carried as a
/// part of its own, what to do if a turn is running, and the effort this
/// turn runs at.
fn turn_start(
    id: u64,
    session: &str,
    text: &str,
    attachments: &[Attachment],
    if_busy: &str,
    effort: Option<&str>,
) -> Value {
    let mut input = vec![json!({ "type": "text", "text": text })];
    input.extend(
        attachments
            .iter()
            .filter(|a| a.is_image())
            .filter_map(image_part),
    );
    let mut params = json!({
        "commandId": command_id(),
        "sessionId": session,
        "input": input,
        "ifBusy": if_busy,
    });
    if let Some(effort) = effort {
        params["reasoningEffort"] = json!(effort);
    }
    jsonrpc_request(id, "turn/start", params)
}

/// The spec a session is revived with, once the token is known to be this
/// adapter's: where it ran, and the model and the effort it ran with.
pub fn revival(adapter_id: &str, token: &ResumeToken) -> Result<SessionSpec, HarnessError> {
    if token.adapter_id != adapter_id {
        return Err(HarnessError::protocol(format!(
            "resume token for {:?} handed to {adapter_id}",
            token.adapter_id
        )));
    }
    Ok(SessionSpec {
        work_item: None,
        cwd: token.cwd.clone(),
        prompt: String::new(),
        model: token.model.clone(),
        effort: token.effort,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Write,
        output_schema: None,
        skills: vec![],
    })
}

/// Queue one message of the protocol. The outbound queue is bounded, and a
/// message it cannot take is a step lost — nothing would ever answer it — so
/// the session ends saying which.
fn queued(shared: &Shared, what: &str, message: Value) -> Result<(), Outcome> {
    shared
        .try_send(OutMsg::Json(message))
        .map_err(|e| Outcome::Failed {
            error: format!("the {what} was not queued: {e}"),
        })
}

/// A session's beginning, as the driver and the facade both read it: whether
/// the session exists yet, and the first prompt kept for it until it does.
#[derive(Debug, Default)]
struct Beginning {
    exists: bool,
    first: Option<(u64, PromptInput)>,
}

type SharedBeginning = Arc<std::sync::Mutex<Beginning>>;

/// What the host asked and waits on, by the request's id — the approval's
/// or the question's — as the facade's `answer` finds it.
#[derive(Debug, Clone)]
enum PendingInput {
    Approval {
        session_id: String,
        approval_id: String,
        requirement: Value,
        choices: Vec<Value>,
    },
    Question {
        session_id: String,
        user_input_id: String,
        questions: Vec<Value>,
    },
}

type PendingInputs = Arc<std::sync::Mutex<HashMap<String, PendingInput>>>;

/// The account usage a live session heard (`usage/changed`), and when: what
/// the adapter answers the footer from before it asks a host.
pub type ObservedUsage = Arc<std::sync::Mutex<Option<(Value, u64)>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Message,
    Reasoning,
    Tool,
    Subagent,
    WebSearch,
}

/// An item under way, by what its start said.
#[derive(Debug, Clone)]
struct KnownItem {
    kind: ItemKind,
    /// A tool's or a sub-agent's name; nothing for words.
    name: String,
    call_id: Option<String>,
    /// Some of its text has been heard as deltas: its end repeats nothing.
    streamed: bool,
}

impl KnownItem {
    fn words(kind: ItemKind) -> Self {
        Self {
            kind,
            name: String::new(),
            call_id: None,
            streamed: false,
        }
    }

    fn named(kind: ItemKind, name: impl Into<String>, call_id: Option<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            call_id,
            streamed: false,
        }
    }
}

/// The token totals the host last said, to take the next delta from.
#[derive(Debug, Default, Clone, Copy)]
struct Totals {
    prompt: u64,
    output: u64,
    cents: u64,
}

fn totals_of(usage: &Value) -> Totals {
    let n = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| usage.get(k).and_then(Value::as_u64))
            .unwrap_or(0)
    };
    let cost = usage.get("cost").and_then(Value::as_f64).unwrap_or(0.0);
    Totals {
        prompt: n(&["promptTokens", "inputTokens"]),
        output: n(&["outputTokens"]),
        cents: (cost * 100.0).round().max(0.0) as u64,
    }
}

struct MspState {
    beginning: SharedBeginning,
    /// The model the session is started on; checked against the host's list.
    model: Option<String>,
    effort: Option<Effort>,
    /// The word every turn carries, once fitted to the model's levels; shared
    /// with the facade.
    effort_word: Arc<std::sync::Mutex<Option<String>>>,
    rows: Vec<ModelRow>,
    /// The session to resume — kept until the host has answered.
    resume: Option<String>,
    cwd: String,
    mcp_servers: Value,
    /// Ids of in-flight `turn/start` requests, shared with the facade.
    turn_ids: Arc<std::sync::Mutex<Vec<u64>>>,
    pending: PendingInputs,
    items: HashMap<String, KnownItem>,
    /// A sub-agent's session, by the child session's id.
    children: HashMap<String, SubagentId>,
    model_seen: Option<String>,
    totals: Totals,
    /// A `session/tokenUsage` was heard this turn: its end's usage is not
    /// counted twice.
    usage_heard: bool,
    observed: Option<ObservedUsage>,
}

/// Open one session: start the host `command` names, shake hands, read its
/// model list, start the session (or resume `resume`), and hand back the
/// session — the one door every MSP session goes through.
pub async fn open(
    command: MspCommand,
    spec: &SessionSpec,
    resume: Option<String>,
    observed: Option<ObservedUsage>,
) -> Result<Box<dyn HarnessSession>, HarnessError> {
    let proc = ProcHandle::spawn(proc_spec(&command, spec))?;
    let (out_tx, out_rx) = mpsc::channel(64);
    let shared = Shared::new(
        out_tx,
        command.adapter_id.clone(),
        spec.model.clone(),
        &spec.cwd,
    )
    .at_effort(spec.effort);
    let revived = resume.is_some();
    shared.broadcaster.emit(SessionEvent::Lifecycle(if revived {
        LifecycleEvent::Revived
    } else {
        LifecycleEvent::Started
    }));

    let turn_ids: Arc<std::sync::Mutex<Vec<u64>>> = Arc::default();
    let pending: PendingInputs = Arc::default();
    let effort_word: Arc<std::sync::Mutex<Option<String>>> = Arc::default();
    let beginning: SharedBeginning = Arc::new(std::sync::Mutex::new(Beginning {
        exists: false,
        first: (!spec.prompt.is_empty()).then(|| {
            (
                ID_FIRST_TURN,
                PromptInput {
                    text: spec.prompt.clone(),
                    attachments: Vec::new(),
                },
            )
        }),
    }));
    let mut state = MspState {
        beginning: Arc::clone(&beginning),
        model: spec.model.clone(),
        effort: spec.effort,
        effort_word: Arc::clone(&effort_word),
        rows: Vec::new(),
        resume,
        cwd: spec.cwd.display().to_string(),
        mcp_servers: mcp_servers_json(&command.adapter_id, &spec.mcp_servers),
        turn_ids: Arc::clone(&turn_ids),
        pending: Arc::clone(&pending),
        items: HashMap::new(),
        children: HashMap::new(),
        model_seen: None,
        totals: Totals::default(),
        usage_heard: false,
        observed,
    };

    // The handshake is queued before the driver starts: written first.
    shared.send(OutMsg::Json(initialize())).await?;

    let shared = shared.in_group(proc.group());
    let driver_shared = shared.for_driver();
    let began = Arc::clone(&beginning);
    let ctx = Arc::clone(&shared.model_ctx);
    tokio::spawn(util::drive_exit(
        proc,
        out_rx,
        driver_shared,
        move |shared, line| {
            let value = match line {
                Line::Json(v) => v,
                Line::Text(t) => {
                    tracing::debug!(target: "bisa_adapters::msp", "non-json stdout: {t}");
                    return Drive::Continue;
                }
            };
            map_msp(shared, &mut state, value)
        },
        move |status, stderr_tail| {
            exit_outcome(status.code(), began.locked().exists, stderr_tail, &ctx)
        },
    ));

    Ok(Box::new(MspSession {
        adapter_id: command.adapter_id,
        shared,
        next_id: AtomicU64::new(FACADE_IDS_FROM),
        turn_ids,
        pending,
        beginning,
        effort_word,
    }))
}

/// The `initialize` request: who the client is, and no capability asked.
fn initialize() -> Value {
    jsonrpc_request(
        ID_INIT,
        "initialize",
        json!({ "clientInfo": { "name": "bisa", "version": env!("CARGO_PKG_VERSION") } }),
    )
}

/// The process `command` starts for `spec`: the program and its words, in
/// the session's directory, with the session's environment — what the spec
/// adds, then what it takes away.
fn proc_spec(command: &MspCommand, spec: &SessionSpec) -> ProcSpec {
    let mut proc_spec = ProcSpec::new(&command.program).cwd(&spec.cwd);
    for arg in &command.args {
        proc_spec = proc_spec.arg(arg);
    }
    for (k, v) in &spec.env {
        proc_spec = proc_spec.env(k, v);
    }
    for k in &spec.env_remove {
        proc_spec = proc_spec.env_remove(k);
    }
    proc_spec
}

/// The JSON-RPC error's code and message, for a response that carries one.
fn error_of(value: &Value) -> Option<(Option<i64>, String)> {
    let error = value.get("error")?;
    let code = error.get("code").and_then(Value::as_i64);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("json-rpc error")
        .to_string();
    Some((code, message))
}

fn str_of<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn map_msp(shared: &Shared, state: &mut MspState, value: Value) -> Drive {
    // A response to one of our requests: an id and no method.
    if let (Some(id), None) = (value.get("id").and_then(Value::as_u64), value.get("method")) {
        return map_response(shared, state, id, value);
    }
    // A request or a notification from the host.
    let parent =
        str_of(&value, "/params/sessionId").and_then(|sid| state.children.get(sid).cloned());
    match value.get("method").and_then(Value::as_str) {
        Some("turn/started") => {
            if parent.is_none() {
                shared.set_phase(Phase::Turn);
            }
            shared.broadcaster.emit(SessionEvent::Progress(
                ProgressEvent::TurnStarted.raised_by(parent),
            ));
        }
        Some("turn/completed") => {
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            if parent.is_some() {
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::TurnEnded.raised_by(parent),
                ));
                return Drive::Continue;
            }
            if !state.usage_heard {
                if let Some(usage) = params.get("usage") {
                    let t = totals_of(usage);
                    shared.add_cost(t.prompt, t.output, t.cents);
                }
            }
            state.usage_heard = false;
            state.items.clear();
            let outcome = turn_outcome(&shared.model_ctx, &params);
            shared
                .broadcaster
                .emit(SessionEvent::Progress(ProgressEvent::TurnEnded));
            shared.set_phase(Phase::Idle);
            shared
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome,
                    is_terminal: false,
                }));
        }
        Some("item/started") => {
            let item = value
                .pointer("/params/item")
                .cloned()
                .unwrap_or(Value::Null);
            state.item_started(shared, &item, parent);
        }
        Some("item/delta") => {
            let params = value.get("params").unwrap_or(&Value::Null);
            let id = str_of(params, "/itemId").unwrap_or("");
            let field = str_of(params, "/field").unwrap_or("");
            let delta = str_of(params, "/delta").unwrap_or("");
            match state.items.get_mut(id) {
                Some(item) if item.kind == ItemKind::Message && field == "text" => {
                    item.streamed = true;
                    shared.broadcaster.emit(SessionEvent::Progress(
                        ProgressEvent::TextDelta {
                            text: delta.to_string(),
                        }
                        .raised_by(parent),
                    ));
                }
                Some(item) if item.kind == ItemKind::Reasoning => {
                    item.streamed = true;
                    shared.broadcaster.emit(SessionEvent::Progress(
                        ProgressEvent::ThinkingDelta {
                            text: delta.to_string(),
                        }
                        .raised_by(parent),
                    ));
                }
                _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
            }
        }
        Some("item/completed") => {
            let item = value
                .pointer("/params/item")
                .cloned()
                .unwrap_or(Value::Null);
            state.item_completed(shared, &item, parent);
        }
        Some("session/tokenUsage") => {
            let params = value.get("params").unwrap_or(&Value::Null);
            state.usage_heard = true;
            let (prompt, output, cents) = match params.get("cumulative").filter(|c| c.is_object()) {
                Some(cumulative) => {
                    let now = totals_of(cumulative);
                    let last = state.totals;
                    state.totals = now;
                    (
                        now.prompt.saturating_sub(last.prompt),
                        now.output.saturating_sub(last.output),
                        now.cents.saturating_sub(last.cents),
                    )
                }
                None => {
                    let t = totals_of(params.get("usage").unwrap_or(&Value::Null));
                    (t.prompt, t.output, t.cents)
                }
            };
            if prompt + output + cents > 0 {
                shared.add_cost(prompt, output, cents);
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::CostDelta {
                        input_tokens: prompt,
                        output_tokens: output,
                        usd_cents: cents,
                    }
                    .raised_by(parent.clone()),
                ));
            }
            if let Some(model) = str_of(params, "/modelId") {
                state.model_moved(shared, model);
            }
        }
        Some("usage/changed") => {
            if let Some(observed) = &state.observed {
                *observed.locked() = Some((
                    value.get("params").cloned().unwrap_or(Value::Null),
                    bisa_harness::usage::now_secs(),
                ));
            }
            shared.broadcaster.emit(SessionEvent::Raw(value));
        }
        Some("session/closed") => {
            let reason = str_of(&value, "/params/reason").unwrap_or("closed");
            if reason == "idle" && !shared.is_ended() {
                return Drive::End(Outcome::Suspended {
                    reason: "the host closed the session (idle); it can be resumed from its token"
                        .to_string(),
                });
            }
            shared.broadcaster.emit(SessionEvent::Raw(value));
        }
        Some("approval/request") | Some("approval/requested") => {
            state.approval_asked(shared, &value, parent);
        }
        Some("userInput/request") | Some("userInput/requested") => {
            state.question_asked(shared, &value, parent);
        }
        _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
    }
    Drive::Continue
}

fn map_response(shared: &Shared, state: &mut MspState, id: u64, value: Value) -> Drive {
    if let Some((code, msg)) = error_of(&value) {
        return match id {
            ID_INIT => Drive::End(shared.model_ctx.outcome(&msg).unwrap_or(Outcome::Failed {
                error: format!("handshake failed: {msg}"),
            })),
            ID_MODELS => {
                // A list the host keeps from this client still runs
                // sessions: the model goes unchecked and the effort is held
                // to every level.
                tracing::warn!(target: "bisa_adapters::msp", "model/list refused: {msg}");
                shared.broadcaster.emit(SessionEvent::Raw(value));
                match state.start_session(shared) {
                    Ok(()) => Drive::Continue,
                    Err(end) => Drive::End(end),
                }
            }
            ID_SESSION => Drive::End(match code {
                Some(-32021) => Outcome::Failed {
                    error: "the session is held by another muse host (sessionInUse): close it and attach again".to_string(),
                },
                Some(-32029) => Outcome::Failed {
                    error: "the host refused the workspace as untrusted".to_string(),
                },
                Some(-32020) => Outcome::Failed {
                    error: format!("the host no longer holds the session: {msg}"),
                },
                _ => shared.model_ctx.outcome(&msg).unwrap_or(Outcome::Failed {
                    error: format!("{} failed: {msg}", state.session_method()),
                }),
            }),
            ID_SET_MODEL => Drive::End(
                shared
                    .model_ctx
                    .typed_outcome(format!("the host refused the model: {msg}"), None),
            ),
            other if state.take_turn_id(other) => {
                let outcome = shared.model_ctx.outcome(&msg).unwrap_or(Outcome::Failed {
                    error: msg.to_string(),
                });
                shared.set_phase(Phase::Idle);
                shared
                    .broadcaster
                    .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                        outcome,
                        is_terminal: false,
                    }));
                Drive::Continue
            }
            _ => {
                tracing::warn!(target: "bisa_adapters::msp", "request {id} failed: {msg}");
                shared.broadcaster.emit(SessionEvent::Raw(value));
                Drive::Continue
            }
        };
    }
    let result = value.get("result").cloned().unwrap_or(Value::Null);
    match id {
        ID_INIT => {
            let version = result.pointer("/schema/version").and_then(Value::as_u64);
            let fingerprint = str_of(&result, "/schema/fingerprint").unwrap_or("");
            if version != Some(SCHEMA_VERSION) || fingerprint != SCHEMA_FINGERPRINT {
                let host = str_of(&result, "/serverInfo/version").unwrap_or("?");
                tracing::warn!(
                    target: "bisa_adapters::msp",
                    host,
                    schema = ?version,
                    fingerprint,
                    "the host's schema is not the one this adapter was written against; read leniently"
                );
            }
            if str_of(&result, "/sessionDurability") == Some("ephemeral") && state.resume.is_some()
            {
                return Drive::End(Outcome::Failed {
                    error: "the host keeps no sessions (ephemeral): nothing to resume".to_string(),
                });
            }
            if let Err(end) = queued(
                shared,
                "initialized notification",
                jsonrpc_notification("initialized", json!({})),
            ) {
                return Drive::End(end);
            }
            if let Err(end) = queued(
                shared,
                "model/list request",
                jsonrpc_request(ID_MODELS, "model/list", json!({})),
            ) {
                return Drive::End(end);
            }
            shared.broadcaster.emit(SessionEvent::Raw(value));
            Drive::Continue
        }
        ID_MODELS => {
            state.rows = rows_of(&result);
            if state.resume.is_none() {
                if let Some(model) = &state.model {
                    if !state.rows.is_empty() && !state.rows.iter().any(|r| &r.id == model) {
                        return Drive::End(shared.model_ctx.typed_outcome(MODEL_NOT_LISTED, None));
                    }
                }
            }
            *state.effort_word.locked() = state
                .effort
                .and_then(|asked| effort_word(&state.rows, state.model.as_deref(), asked));
            match state.start_session(shared) {
                Ok(()) => Drive::Continue,
                Err(end) => Drive::End(end),
            }
        }
        ID_SESSION => {
            let resumed = state.resume.take();
            let said = str_of(&result, "/session/sessionId");
            if let Some(sid) = said.or(resumed.as_deref()) {
                shared.set_native_id(sid);
            }
            // The past, never said again as if it were happening now.
            for item in result
                .pointer("/history/items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                shared.broadcaster.emit(SessionEvent::Raw(item.clone()));
            }
            if let Some(last) = result.get("lastTurn") {
                shared.broadcaster.emit(SessionEvent::Raw(last.clone()));
            }
            let on = str_of(&result, "/session/modelId").map(str::to_string);
            if let Some(model) = &on {
                state.model_moved(shared, model);
            }
            // A resumed session that is not on the model its token names is
            // set back to it before its first turn.
            if resumed.is_some() {
                if let (Some(asked), Some(sid)) = (state.model.clone(), shared.native_id()) {
                    if on.as_deref() != Some(asked.as_str()) {
                        let set = jsonrpc_request(
                            ID_SET_MODEL,
                            "session/setModel",
                            json!({ "commandId": command_id(), "sessionId": sid, "model": { "modelId": asked } }),
                        );
                        return match queued(shared, "model", set) {
                            Ok(()) => Drive::Continue,
                            Err(end) => Drive::End(end),
                        };
                    }
                }
            }
            match state.begin(shared) {
                Ok(()) => Drive::Continue,
                Err(end) => Drive::End(end),
            }
        }
        ID_SET_MODEL => {
            if let Some(model) = state.model.clone() {
                state.model_moved(shared, &model);
            }
            match state.begin(shared) {
                Ok(()) => Drive::Continue,
                Err(end) => Drive::End(end),
            }
        }
        other if state.take_turn_id(other) => {
            // The ack: started, queued or steered. A turn that started is
            // running; one queued waits for the running turn's end.
            if str_of(&result, "/disposition") == Some("started")
                || result.get("startedNewTurn").and_then(Value::as_bool) == Some(true)
            {
                shared.set_phase(Phase::Turn);
            }
            shared.broadcaster.emit(SessionEvent::Raw(value));
            Drive::Continue
        }
        _ => {
            shared.broadcaster.emit(SessionEvent::Raw(value));
            Drive::Continue
        }
    }
}

impl MspState {
    fn session_method(&self) -> &'static str {
        if self.resume.is_some() {
            "session/resume"
        } else {
            "session/start"
        }
    }

    fn take_turn_id(&self, id: u64) -> bool {
        let mut ids = self.turn_ids.locked();
        let found = ids.contains(&id);
        ids.retain(|p| *p != id);
        found
    }

    /// The session command: a start in the session's folder on its model,
    /// under the approval mode, with its servers — or a resume by id.
    fn start_session(&mut self, shared: &Shared) -> Result<(), Outcome> {
        let mut params = json!({ "commandId": command_id() });
        if let Some(sid) = &self.resume {
            params["sessionId"] = json!(sid);
            params["excludeItems"] = json!(true);
        } else {
            params["workspaceRoot"] = json!(self.cwd);
            params["approvalMode"] = json!(APPROVAL_MODE);
            if let Some(model) = &self.model {
                params["modelId"] = json!(model);
            }
        }
        if self.mcp_servers.as_object().is_some_and(|m| !m.is_empty()) {
            params["config"] = json!({ "mcpServers": self.mcp_servers });
        }
        let method = self.session_method();
        queued(shared, method, jsonrpc_request(ID_SESSION, method, params))
    }

    /// The session exists: the prompt kept for it goes out now, and the
    /// phase moves under the same lock the facade reads it behind.
    fn begin(&mut self, shared: &Shared) -> Result<(), Outcome> {
        let first = {
            let mut beginning = self.beginning.locked();
            beginning.exists = true;
            let first = beginning.first.take();
            shared.set_phase(Phase::Idle);
            first
        };
        let Some((request, input)) = first else {
            return Ok(());
        };
        let Some(sid) = shared.native_id() else {
            return Ok(());
        };
        self.turn_ids.locked().push(request);
        shared.set_phase(Phase::Turn);
        let effort = self.effort_word.locked().clone();
        let first = turn_start(
            request,
            &sid,
            &input.text,
            &input.attachments,
            "queue",
            effort.as_deref(),
        );
        queued(shared, "first turn", first)
    }

    fn model_moved(&mut self, shared: &Shared, model: &str) {
        if self.model_seen.as_deref() == Some(model) {
            return;
        }
        self.model_seen = Some(model.to_string());
        shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::ModelChanged {
                model: model.to_string(),
            }));
    }

    fn remember(&mut self, id: &str, item: KnownItem) {
        if self.items.len() < MAX_KNOWN_ITEMS || self.items.contains_key(id) {
            self.items.insert(id.to_string(), item);
        }
    }

    fn item_started(&mut self, shared: &Shared, item: &Value, parent: Option<SubagentId>) {
        let id = str_of(item, "/itemId").unwrap_or("").to_string();
        match str_of(item, "/kind") {
            Some("agentMessage") => self.remember(&id, KnownItem::words(ItemKind::Message)),
            Some("reasoning") => self.remember(&id, KnownItem::words(ItemKind::Reasoning)),
            Some("toolCall") => {
                let tool = str_of(item, "/tool").unwrap_or("?").to_string();
                let call_id = str_of(item, "/callId").map(str::to_string);
                let tier = tier_of(&tool);
                let args = args_value(item.get("args").unwrap_or(&Value::Null));
                shared.set_activity(tool.clone());
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::ToolStarted {
                        name: tool.clone(),
                        args_summary: util::summarize_args(&args, ARGS_SUMMARY),
                        tier,
                        id: call_id.clone().or_else(|| Some(id.clone())),
                    }
                    .raised_by(parent),
                ));
                self.remember(&id, KnownItem::named(ItemKind::Tool, tool, call_id));
            }
            Some("subagent") => {
                let sub = SubagentId(str_of(item, "/subagentId").unwrap_or(&id).to_string());
                if let Some(child) = str_of(item, "/childSessionId") {
                    self.children.insert(child.to_string(), sub.clone());
                }
                let role = str_of(item, "/role").unwrap_or("subagent").to_string();
                let objective = str_of(item, "/objective").unwrap_or("");
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::SubagentStarted {
                        id: sub,
                        name: role.clone(),
                        description: util::summarize_args(&json!(objective), ARGS_SUMMARY),
                    }
                    .raised_by(parent),
                ));
                self.remember(&id, KnownItem::named(ItemKind::Subagent, role, None));
            }
            Some("webSearch") => {
                shared.set_activity("web_search");
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::ToolStarted {
                        name: "web_search".to_string(),
                        args_summary: util::summarize_args(
                            item.get("query").unwrap_or(&Value::Null),
                            ARGS_SUMMARY,
                        ),
                        tier: ToolTier::Read,
                        id: Some(id.clone()),
                    }
                    .raised_by(parent),
                ));
                self.remember(
                    &id,
                    KnownItem::named(ItemKind::WebSearch, "web_search", None),
                );
            }
            _ => shared.broadcaster.emit(SessionEvent::Raw(item.clone())),
        }
    }

    fn item_completed(&mut self, shared: &Shared, item: &Value, parent: Option<SubagentId>) {
        let id = str_of(item, "/itemId").unwrap_or("");
        let Some(known) = self.items.remove(id) else {
            shared.broadcaster.emit(SessionEvent::Raw(item.clone()));
            return;
        };
        let ok = str_of(item, "/status") == Some("completed");
        match known.kind {
            ItemKind::Message | ItemKind::Reasoning => {
                let text = str_of(item, "/text").unwrap_or("");
                if !known.streamed && !text.is_empty() {
                    let event = if known.kind == ItemKind::Message {
                        ProgressEvent::TextDelta {
                            text: text.to_string(),
                        }
                    } else {
                        ProgressEvent::ThinkingDelta {
                            text: text.to_string(),
                        }
                    };
                    shared
                        .broadcaster
                        .emit(SessionEvent::Progress(event.raised_by(parent)));
                }
            }
            ItemKind::Tool | ItemKind::WebSearch => {
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::ToolEnded {
                        name: known.name,
                        ok,
                        id: known.call_id.or_else(|| Some(id.to_string())),
                    }
                    .raised_by(parent),
                ));
            }
            ItemKind::Subagent => {
                let sub = SubagentId(str_of(item, "/subagentId").unwrap_or(id).to_string());
                if let Some(child) = str_of(item, "/childSessionId") {
                    self.children.remove(child);
                }
                shared.broadcaster.emit(SessionEvent::Progress(
                    ProgressEvent::SubagentEnded { id: sub, ok }.raised_by(parent),
                ));
            }
        }
    }

    /// The host asks before a tool runs: the request is acknowledged at
    /// once, the choices kept, and the engine told — once, whichever of the
    /// two frames comes first.
    fn approval_asked(&mut self, shared: &Shared, value: &Value, parent: Option<SubagentId>) {
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        if let Some(id) = value.get("id") {
            if let Err(e) = shared.try_send(OutMsg::Json(jsonrpc_response(id, json!({})))) {
                tracing::warn!(target: "bisa_adapters::msp", "the approval's acknowledgement was not queued: {e}");
            }
        }
        let approval_id = str_of(&params, "/approvalId").unwrap_or("").to_string();
        if approval_id.is_empty() || self.pending.locked().contains_key(&approval_id) {
            shared.broadcaster.emit(SessionEvent::Raw(value.clone()));
            return;
        }
        let tool = str_of(&params, "/toolName").unwrap_or("?").to_string();
        let raw = args_value(params.get("rawArgs").unwrap_or(&Value::Null));
        let choices = params
            .get("availableChoices")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.pending.locked().insert(
            approval_id.clone(),
            PendingInput::Approval {
                session_id: str_of(&params, "/sessionId").unwrap_or("").to_string(),
                approval_id: approval_id.clone(),
                requirement: params
                    .get("currentRequirementId")
                    .cloned()
                    .unwrap_or_else(|| json!({ "approvalId": approval_id, "sourceIndex": 0 })),
                choices,
            },
        );
        shared.set_activity(format!("asked to run {tool}"));
        shared.set_phase(Phase::AwaitingInput);
        shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request: InputRequest::permission(
                    approval_id,
                    tool.clone(),
                    tier_of(&tool),
                    util::summarize_args(&raw, ARGS_SUMMARY),
                    raw,
                )
                .raised_by(parent),
            }));
    }

    /// The agent asks the person something: the first question, its options
    /// as the labels the host offers.
    fn question_asked(&mut self, shared: &Shared, value: &Value, parent: Option<SubagentId>) {
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        if let Some(id) = value.get("id") {
            if let Err(e) = shared.try_send(OutMsg::Json(jsonrpc_response(id, json!({})))) {
                tracing::warn!(target: "bisa_adapters::msp", "the question's acknowledgement was not queued: {e}");
            }
        }
        let user_input_id = str_of(&params, "/userInputId").unwrap_or("").to_string();
        if user_input_id.is_empty() || self.pending.locked().contains_key(&user_input_id) {
            shared.broadcaster.emit(SessionEvent::Raw(value.clone()));
            return;
        }
        let questions = params
            .get("questions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let first = questions.first().cloned().unwrap_or(Value::Null);
        let text = str_of(&first, "/question")
            .or_else(|| str_of(&first, "/header"))
            .unwrap_or("The agent asks for your input.")
            .to_string();
        let options: Vec<String> = first
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| str_of(o, "/label"))
            .map(str::to_string)
            .collect();
        self.pending.locked().insert(
            user_input_id.clone(),
            PendingInput::Question {
                session_id: str_of(&params, "/sessionId").unwrap_or("").to_string(),
                user_input_id: user_input_id.clone(),
                questions,
            },
        );
        shared.set_activity("asked a question");
        shared.set_phase(Phase::AwaitingInput);
        shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request: InputRequest::question(user_input_id, text, options).raised_by(parent),
            }));
    }
}

pub struct MspSession {
    adapter_id: String,
    shared: Shared,
    next_id: AtomicU64,
    turn_ids: Arc<std::sync::Mutex<Vec<u64>>>,
    pending: PendingInputs,
    beginning: SharedBeginning,
    effort_word: Arc<std::sync::Mutex<Option<String>>>,
}

impl MspSession {
    /// A `turn/start` of the facade's: queued under `if_busy`, its id kept
    /// so the ack and an error find it.
    async fn turn(
        &self,
        text: &str,
        attachments: &[Attachment],
        if_busy: &str,
    ) -> Result<(), HarnessError> {
        if self.shared.phase() == Phase::Ended {
            return Err(HarnessError::Terminated);
        }
        let sid = self.shared.native_id().ok_or(HarnessError::Busy)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.turn_ids.locked().push(id);
        let effort = self.effort_word.locked().clone();
        self.shared
            .send(OutMsg::Json(turn_start(
                id,
                &sid,
                text,
                attachments,
                if_busy,
                effort.as_deref(),
            )))
            .await
    }
}

#[async_trait]
impl HarnessSession for MspSession {
    fn snapshot(&self) -> SessionSnapshot {
        self.shared.snapshot()
    }

    fn phase(&self) -> Phase {
        self.shared.phase()
    }

    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        {
            // A session that does not exist yet keeps its first prompt; the
            // driver sends it once the host has answered. One may wait —
            // and a session that will never exist takes it too: how it ended
            // is the last word of its stream.
            let mut beginning = self.beginning.locked();
            if !beginning.exists {
                if beginning.first.is_some() {
                    return Err(HarnessError::Busy);
                }
                let request = self.next_id.fetch_add(1, Ordering::Relaxed);
                beginning.first = Some((request, input));
                return Ok(());
            }
        }
        if self.shared.phase() == Phase::Ended {
            return Err(HarnessError::Terminated);
        }
        if self.shared.phase() == Phase::Turn {
            return Err(HarnessError::Busy);
        }
        self.shared.set_phase(Phase::Turn);
        self.turn(&input.text, &input.attachments, "queue").await
    }

    async fn steer(&self, msg: Steer) -> Result<(), HarnessError> {
        self.turn(&msg.text, &msg.attachments, "steer").await
    }

    async fn follow_up(&self, msg: Steer) -> Result<(), HarnessError> {
        self.turn(&msg.text, &msg.attachments, "queue").await
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        // Told the way it understands first — the turn cancelled — then its
        // stdin closed and its whole group given the grace to leave.
        if let Some(sid) = self.shared.native_id() {
            let cancel = jsonrpc_request(
                self.next_id.fetch_add(1, Ordering::Relaxed),
                "turn/cancel",
                json!({ "commandId": command_id(), "sessionId": sid }),
            );
            if let Err(e) = self.shared.send(OutMsg::Json(cancel)).await {
                tracing::debug!("the driver had already ended: {e}");
            }
        }
        self.shared.end(Outcome::Aborted);
        self.shared
            .abort_group(bisa_harness::proc::ABORT_GRACE)
            .await;
        Ok(())
    }

    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        let pending = self.pending.locked().remove(request_id).ok_or_else(|| {
            HarnessError::protocol(format!("no input request {request_id} is waiting"))
        })?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let message = match pending {
            PendingInput::Approval {
                session_id,
                approval_id,
                requirement,
                choices,
            } => {
                let (allow, feedback) = match &answer {
                    InputAnswer::Deny { reason } => (false, Some(reason.clone())),
                    _ => (true, None),
                };
                let choice = choice_for(&choices, allow).ok_or_else(|| {
                    HarnessError::protocol(format!(
                        "the host offered no choice that {} the call",
                        if allow { "allows" } else { "refuses" }
                    ))
                })?;
                let mut params = json!({
                    "commandId": command_id(),
                    "sessionId": session_id,
                    "approvalId": approval_id,
                    "requirementId": requirement,
                    "choiceId": choice.get("choiceId").cloned().unwrap_or(Value::Null),
                });
                if choice.get("acceptsFeedback").and_then(Value::as_bool) == Some(true) {
                    if let Some(feedback) = feedback {
                        params["feedback"] = json!(feedback);
                    }
                }
                jsonrpc_request(id, "approval/decide", params)
            }
            PendingInput::Question {
                session_id,
                user_input_id,
                questions,
            } => {
                let text = match answer {
                    InputAnswer::Text { text } => text,
                    InputAnswer::Deny { reason } => reason,
                    InputAnswer::Allow { .. } => String::new(),
                };
                let first = questions.first().cloned().unwrap_or(Value::Null);
                let question_id = str_of(&first, "/id").unwrap_or("").to_string();
                let labelled = first
                    .get("options")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|o| str_of(o, "/label") == Some(text.as_str()));
                let mut one = json!({ "questionId": question_id });
                if labelled {
                    one["selectedLabel"] = json!(text);
                } else {
                    one["freeText"] = json!(text);
                }
                jsonrpc_request(
                    id,
                    "userInput/answer",
                    json!({
                        "commandId": command_id(),
                        "sessionId": session_id,
                        "userInputId": user_input_id,
                        "answers": [one],
                    }),
                )
            }
        };
        self.shared.send(OutMsg::Json(message)).await?;
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                id: request_id.to_string(),
            }));
        Ok(())
    }

    fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.shared.resume_token(&self.adapter_id)
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        // Graceful: EOF lets the host drain and exit; one that ignores it is
        // ended with its group past the grace.
        self.shared.close_stdin_quietly().await;
        self.shared
            .leave_or_kill(bisa_harness::proc::DISPOSE_GRACE)
            .await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// A host asked one question and nothing else

/// Why a short-lived host did not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRefusal {
    /// The host could not be started: not on `PATH`, or the spawn failed.
    NotStarted(String),
    /// The host answered the handshake or the question with an error.
    Refused { code: Option<i64>, message: String },
    /// The host left before answering — with the exit's meaning, when the
    /// manual gives one.
    Ended(String),
    /// The host did not answer within the budget (seconds).
    TimedOut(u64),
}

impl std::fmt::Display for HostRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostRefusal::NotStarted(why) => write!(f, "the host could not be started: {why}"),
            HostRefusal::Refused {
                code: Some(code),
                message,
            } => {
                write!(f, "the host refused ({code}): {message}")
            }
            HostRefusal::Refused {
                code: None,
                message,
            } => write!(f, "the host refused: {message}"),
            HostRefusal::Ended(why) => write!(f, "the host ended before answering: {why}"),
            HostRefusal::TimedOut(secs) => write!(f, "the host did not answer within {secs} s"),
        }
    }
}

/// How long a short-lived host has to leave on EOF before it is killed.
const ASK_LEAVE: Duration = Duration::from_secs(2);

/// Start `program args` as a host, shake hands, ask `method` once, and let
/// it go: the answer's `result`. The host opens no session, so it is given
/// no trust and writes nothing of a session's.
pub async fn ask(
    program: &str,
    args: &[&str],
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, HostRefusal> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            HostRefusal::NotStarted(format!("{program} not found on PATH"))
        } else {
            HostRefusal::NotStarted(e.to_string())
        }
    })?;
    let asked = tokio::time::timeout(timeout, talk(&mut child, method, params)).await;
    // EOF was given inside `talk`; a host that lingers past the grace goes.
    if tokio::time::timeout(ASK_LEAVE, child.wait()).await.is_err() {
        if let Err(e) = child.kill().await {
            tracing::warn!("the host did not end on request: {e}"); // LCOV_EXCL_LINE: a kill the kernel refuses on a child of ours; nothing says why
        }
    }
    match asked {
        Ok(answer) => answer,
        Err(_) => Err(HostRefusal::TimedOut(timeout.as_secs())),
    }
}

const ASK_ID: u64 = 2;

/// The handshake and the one question, answered by id; the host's stdin
/// closed after.
async fn talk(
    child: &mut tokio::process::Child,
    method: &str,
    params: Value,
) -> Result<Value, HostRefusal> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| HostRefusal::NotStarted("the host has no stdin".into()))?; // LCOV_EXCL_LINE: both pipes are asked for at the spawn; the arm keeps the Option total
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HostRefusal::NotStarted("the host has no stdout".into()))?; // LCOV_EXCL_LINE: both pipes are asked for at the spawn; the arm keeps the Option total
    let mut reader = tokio::io::BufReader::new(stdout);
    // Only `initialize` before the `initialized` notification: the host
    // refuses everything else until then.
    write_frame(&mut stdin, initialize()).await?;
    read_answer(&mut reader, child, ID_INIT).await?;
    write_frame(&mut stdin, jsonrpc_notification("initialized", json!({}))).await?;
    write_frame(&mut stdin, jsonrpc_request(ASK_ID, method, params)).await?;
    let result = read_answer(&mut reader, child, ASK_ID).await?;
    drop(stdin);
    Ok(result)
}

/// One frame on the host's stdin; a stdin the host closed is the host gone.
async fn write_frame(
    stdin: &mut tokio::process::ChildStdin,
    frame: Value,
) -> Result<(), HostRefusal> {
    use tokio::io::AsyncWriteExt;
    stdin
        .write_all(format!("{frame}\n").as_bytes())
        .await
        .map_err(|_| HostRefusal::Ended("the host closed its stdin".into()))
}

/// The next frame of the host's that answers `id`: its result, or its
/// error as a refusal; stdout closing before it is the host leaving.
async fn read_answer<R>(
    reader: &mut R,
    child: &mut tokio::process::Child,
    id: u64,
) -> Result<Value, HostRefusal>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    while let Ok(Some(read)) =
        bisa_harness::proc::next_line(reader, bisa_harness::proc::MAX_LINE_BYTES).await
    {
        let Ok(v) = serde_json::from_str::<Value>(&read.text) else {
            continue;
        };
        if v.get("id").and_then(Value::as_u64) != Some(id) || v.get("method").is_some() {
            continue;
        }
        if let Some((code, message)) = error_of(&v) {
            return Err(HostRefusal::Refused { code, message });
        }
        return Ok(v.get("result").cloned().unwrap_or(Value::Null));
    }
    // Its output ended: the exit, which the pipe closes a moment before,
    // says why — given the leaving grace to be reaped.
    let why = match tokio::time::timeout(ASK_LEAVE, child.wait()).await {
        Ok(Ok(status)) => status
            .code()
            .and_then(exit_words)
            .map(str::to_string)
            .unwrap_or_else(|| format!("exit {}", status.code().unwrap_or(-1))),
        _ => "its output ended".to_string(), // LCOV_EXCL_LINE: a child whose pipe closed yet lingers past the grace, or a wait the kernel refuses; no stub stages either
    };
    Err(HostRefusal::Ended(why))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::McpProvenance;
    use bisa_harness::Attachment;

    fn row(id: &str, default: bool, variants: &[&str]) -> Value {
        json!({ "modelId": id, "displayLabel": format!("Model {id}"), "isDefault": default, "variants": variants })
    }

    #[test]
    fn a_command_id_is_a_version_seven_uuid_and_two_differ() {
        let a = command_id();
        let b = command_id();
        assert_ne!(a, b);
        for id in [&a, &b] {
            assert_eq!(id.len(), 36, "{id}");
            let parts: Vec<&str> = id.split('-').collect();
            assert_eq!(
                parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
                [8, 4, 4, 4, 12],
                "{id}"
            );
            assert!(
                id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()),
                "{id}"
            );
            assert_eq!(&parts[2][..1], "7", "the version nibble: {id}");
            assert!(
                matches!(&parts[3][..1], "8" | "9" | "a" | "b"),
                "the variant: {id}"
            );
        }
        // The first 48 bits are the time, so ids minted in order sort in order.
        assert!(a[..13] <= b[..13]);
    }

    #[test]
    fn the_rows_are_the_hosts_models_the_default_first_each_once_with_the_levels_their_variants_name(
    ) {
        let result = json!({ "models": [
            row("small", false, &["low", "none", "high", "ultra"]),
            row("large", true, &["minimal", "medium", "xhigh", "max"]),
            row("small", false, &["low"]),
            { "modelId": "bare", "displayLabel": "", "isDefault": false, "variants": "unknown" },
            { "displayLabel": "no id" },
        ]});
        let rows = rows_of(&result);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["large", "small", "bare"]
        );
        assert_eq!(
            rows[0].variants,
            [Effort::Minimal, Effort::Medium, Effort::Xhigh, Effort::Max]
        );
        assert_eq!(
            rows[1].variants,
            [Effort::Low, Effort::High],
            "none and ultra are dropped"
        );
        assert!(rows[2].variants.is_empty());
        assert_eq!(rows[2].label, None, "an empty label is none");
        assert_eq!(rows[0].label.as_deref(), Some("Model large"));
        let models = listed(&rows);
        assert_eq!(models[0].efforts, rows[0].variants);
        assert_eq!(
            models[2].efforts, LEVELS,
            "a model whose levels are unknown takes every level"
        );
        assert!(rows_of(&json!({})).is_empty());
    }

    #[test]
    fn the_effort_word_is_fitted_to_the_models_variants_and_to_the_six_for_a_model_nobody_lists() {
        let rows = rows_of(
            &json!({ "models": [row("small", false, &["low", "high"]), row("large", true, &["medium", "max"])] }),
        );
        assert_eq!(levels_of(&rows, Some("small")), [Effort::Low, Effort::High]);
        assert_eq!(
            levels_of(&rows, None),
            [Effort::Medium, Effort::Max],
            "the default row for no model"
        );
        assert_eq!(
            levels_of(&rows, Some("huge")),
            LEVELS,
            "a model nobody lists takes every level"
        );
        assert_eq!(levels_of(&[], None), LEVELS);
        assert_eq!(
            effort_word(&rows, Some("small"), Effort::Xhigh).as_deref(),
            Some("high")
        );
        assert_eq!(
            effort_word(&rows, Some("small"), Effort::Minimal).as_deref(),
            Some("low")
        );
        assert_eq!(
            effort_word(&rows, Some("huge"), Effort::Xhigh).as_deref(),
            Some("xhigh")
        );
        assert_eq!(
            effort_word(&rows, None, Effort::Low).as_deref(),
            Some("medium")
        );
        let no_default = rows_of(&json!({ "models": [row("only", false, &["low"])] }));
        assert_eq!(
            levels_of(&no_default, None),
            [Effort::Low],
            "the first row when none is the default"
        );
    }

    #[test]
    fn the_servers_are_an_object_by_name_with_env_as_an_object_and_cwd_and_sse_dropped() {
        let mounts = vec![
            McpMount {
                config: McpServerConfig::Stdio {
                    name: "bisa".into(),
                    command: "bisa".into(),
                    args: vec!["mcp".into()],
                    env: [("BISA_SESSION".to_string(), "01S".to_string())]
                        .into_iter()
                        .collect(),
                    cwd: Some("/elsewhere".into()),
                },
                provenance: McpProvenance::Platform,
            },
            McpMount {
                config: McpServerConfig::Http {
                    name: "docs".into(),
                    url: "https://docs.example/mcp".into(),
                    headers: [("Authorization".to_string(), "Bearer x".to_string())]
                        .into_iter()
                        .collect(),
                },
                provenance: McpProvenance::Platform,
            },
            McpMount {
                config: McpServerConfig::Sse {
                    name: "old".into(),
                    url: "https://old.example/sse".into(),
                    headers: Default::default(),
                },
                provenance: McpProvenance::Platform,
            },
        ];
        let servers = mcp_servers_json("muse-code", &mounts);
        assert_eq!(
            servers,
            json!({
                "bisa": { "transport": "stdio", "command": "bisa", "args": ["mcp"], "env": { "BISA_SESSION": "01S" } },
                "docs": { "transport": "streamableHttp", "url": "https://docs.example/mcp", "headers": { "Authorization": "Bearer x" } },
            })
        );
        assert_eq!(mcp_servers_json("muse-code", &[]), json!({}));
    }

    #[test]
    fn a_tools_arguments_are_its_json_when_they_parse_and_its_tier_is_by_its_neutral_name() {
        assert_eq!(
            args_value(&json!(r#"{"path":"Cargo.toml"}"#)),
            json!({ "path": "Cargo.toml" })
        );
        assert_eq!(args_value(&json!("ls -la")), json!("ls -la"));
        assert_eq!(args_value(&json!({ "a": 1 })), json!({ "a": 1 }));
        assert_eq!(tier_of("read_file"), ToolTier::Read);
        assert_eq!(tier_of("write_file"), ToolTier::Write);
        assert_eq!(tier_of("WebSearch"), ToolTier::Read);
        assert_eq!(tier_of("bash"), ToolTier::Exec);
        assert_eq!(tier_of("mcp__bisa__list_items"), ToolTier::Read);
    }

    #[test]
    fn a_choice_is_the_once_one_never_for_the_session_nor_an_amendment_and_none_is_refused() {
        let choices = vec![
            json!({ "choiceId": "allow_session", "decision": "approvedForSession", "scope": "session" }),
            json!({ "choiceId": "allow_always", "decision": "approvedPolicyAmendment", "scope": "persistent" }),
            json!({ "choiceId": "allow_once", "decision": "approved", "scope": "once" }),
            json!({ "choiceId": "abort", "decision": "abort", "scope": "once", "acceptsFeedback": true }),
        ];
        assert_eq!(
            choice_for(&choices, true).unwrap()["choiceId"],
            "allow_once"
        );
        assert_eq!(
            choice_for(&choices, false).unwrap()["choiceId"],
            "abort",
            "no denied choice: abort"
        );
        let with_deny = [
            choices.clone(),
            vec![json!({ "choiceId": "deny_once", "decision": "denied" })],
        ]
        .concat();
        assert_eq!(
            choice_for(&with_deny, false).unwrap()["choiceId"],
            "deny_once"
        );
        let only_session =
            vec![json!({ "choiceId": "allow_session", "decision": "approvedForSession" })];
        assert!(
            choice_for(&only_session, true).is_none(),
            "a standing grant is never chosen"
        );
        assert!(choice_for(&[], false).is_none());
    }

    #[test]
    fn a_turns_end_is_read_typed_a_quota_or_a_retryable_model_error_a_wall_the_rest_failed_in_words(
    ) {
        let ctx = ModelCtx::new("muse-code", Some("large".into()));
        assert!(matches!(
            turn_outcome(&ctx, &json!({ "terminal": "completed" })),
            Outcome::Completed
        ));
        assert!(matches!(
            turn_outcome(&ctx, &json!({ "terminal": "cancelled" })),
            Outcome::Aborted
        ));
        match turn_outcome(
            &ctx,
            &json!({ "terminal": "failed", "error": { "kind": "modelError", "message": "too many requests", "retryable": true, "quota": { "resetsAtMs": 1_788_782_400_000u64 } } }),
        ) {
            Outcome::ModelUnavailable {
                model,
                reason,
                retry_after,
            } => {
                assert_eq!(model, "large");
                assert_eq!(reason, "modelError: too many requests");
                assert_eq!(retry_after, Some(1_788_782_400));
            }
            other => panic!("{other:?}"),
        }
        match turn_outcome(
            &ctx,
            &json!({ "terminal": "failed", "error": { "kind": "modelError", "message": "upstream hiccup", "retryable": true } }),
        ) {
            Outcome::ModelUnavailable { retry_after, .. } => assert_eq!(retry_after, None),
            other => panic!("{other:?}"),
        }
        match turn_outcome(
            &ctx,
            &json!({ "terminal": "failed", "error": { "kind": "authRequired", "message": "no credential", "retryable": false } }),
        ) {
            Outcome::Failed { error } => {
                assert!(error.contains("run `muse` and sign in"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        match turn_outcome(
            &ctx,
            &json!({ "terminal": "failed", "error": { "kind": "stepLimit", "message": "30 steps", "retryable": false } }),
        ) {
            Outcome::Failed { error } => assert_eq!(error, "stepLimit: 30 steps"),
            other => panic!("{other:?}"),
        }
        match turn_outcome(
            &ctx,
            &json!({ "terminal": "failed", "reason": "crash_inferred" }),
        ) {
            Outcome::Failed { error } => assert_eq!(error, "failed: crash_inferred"),
            other => panic!("{other:?}"),
        }
        match turn_outcome(&ctx, &json!({ "terminal": "failed" })) {
            Outcome::Failed { error } => assert_eq!(error, "failed: the turn failed"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_exit_is_read_by_its_code_and_whether_a_session_existed() {
        let ctx = ModelCtx::new("muse-code", None);
        let tail = vec!["warming up".to_string(), "bye".to_string()];
        assert!(matches!(
            exit_outcome(Some(0), true, &tail, &ctx),
            Outcome::Completed
        ));
        match exit_outcome(Some(0), false, &tail, &ctx) {
            Outcome::Failed { error } => assert_eq!(
                error,
                "the host ended before its session began; stderr tail: warming up | bye"
            ),
            other => panic!("{other:?}"),
        }
        match exit_outcome(Some(0), false, &[], &ctx) {
            Outcome::Failed { error } => {
                assert_eq!(error, "the host ended before its session began")
            }
            other => panic!("{other:?}"),
        }
        for (code, word) in [
            (2, "exit 2"),
            (3, "sign in"),
            (4, "another muse host"),
            (5, "MUSE_EXPERIMENTAL_SDK_ENABLED"),
        ] {
            match exit_outcome(Some(code), true, &tail, &ctx) {
                Outcome::Failed { error } => assert!(error.contains(word), "{code}: {error}"),
                other => panic!("{code}: {other:?}"),
            }
        }
        match exit_outcome(Some(143), true, &[], &ctx) {
            Outcome::Failed { error } => assert_eq!(error, "the host was interrupted (exit 143)"),
            other => panic!("{other:?}"),
        }
        match exit_outcome(Some(7), true, &tail, &ctx) {
            Outcome::Failed { error } => assert_eq!(
                error,
                "process exited with 7; stderr tail: warming up | bye"
            ),
            other => panic!("{other:?}"),
        }
        match exit_outcome(None, true, &[], &ctx) {
            Outcome::Failed { error } => assert_eq!(error, "process ended by a signal"),
            other => panic!("{other:?}"),
        }
        // Prose on the tail still names a wall, for a code the manual does not.
        let walled = exit_outcome(Some(1), true, &["rate limit exceeded".to_string()], &ctx);
        assert!(
            matches!(walled, Outcome::ModelUnavailable { .. })
                || matches!(walled, Outcome::Failed { .. })
        );
    }

    #[test]
    fn a_turn_carries_its_text_its_images_the_busy_word_and_the_effort() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("shot.png");
        std::fs::write(&png, b"\x89PNG").unwrap();
        let attachments = vec![
            Attachment {
                name: "shot.png".into(),
                mime: "image/png".into(),
                path: png,
            },
            Attachment {
                name: "notes.txt".into(),
                mime: "text/plain".into(),
                path: dir.path().join("notes.txt"),
            },
            Attachment {
                name: "gone.png".into(),
                mime: "image/png".into(),
                path: dir.path().join("gone.png"),
            },
        ];
        let frame = turn_start(7, "s-1", "look", &attachments, "steer", Some("high"));
        assert_eq!(frame["id"], 7);
        assert_eq!(frame["method"], "turn/start");
        let params = &frame["params"];
        assert_eq!(params["sessionId"], "s-1");
        assert_eq!(params["ifBusy"], "steer");
        assert_eq!(params["reasoningEffort"], "high");
        assert_eq!(params["commandId"].as_str().unwrap().len(), 36);
        let input = params["input"].as_array().unwrap();
        assert_eq!(
            input.len(),
            2,
            "the text and the one image that could be read: {input:?}"
        );
        assert_eq!(input[0], json!({ "type": "text", "text": "look" }));
        assert_eq!(input[1]["type"], "image");
        assert_eq!(input[1]["mediaType"], "image/png");
        assert_eq!(input[1]["base64Data"], "iVBORw==");
        let bare = turn_start(8, "s-1", "go", &[], "queue", None);
        assert!(bare["params"].get("reasoningEffort").is_none());
        assert_eq!(bare["params"]["input"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_token_revives_only_the_adapter_that_minted_it_with_what_it_ran() {
        let token = ResumeToken {
            adapter_id: "muse-code".into(),
            native_id: "s-1".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: Some("large".into()),
            effort: Some(Effort::Xhigh),
        };
        let spec = revival("muse-code", &token).unwrap();
        assert_eq!(spec.cwd, token.cwd);
        assert_eq!(spec.model.as_deref(), Some("large"));
        assert_eq!(spec.effort, Some(Effort::Xhigh));
        assert!(spec.prompt.is_empty());
        assert!(matches!(
            revival("grok", &token),
            Err(HarnessError::Protocol(_))
        ));
        let command = MspCommand::new("muse-code", "muse", ["serve", "--trust-workspace"]);
        let mut spec = spec;
        spec.env.insert("KEPT".into(), "1".into());
        spec.env_remove.push("TAKEN_AWAY".into());
        let proc = proc_spec(&command, &spec);
        assert_eq!(proc.program, "muse");
        assert_eq!(proc.args, ["serve", "--trust-workspace"]);
        assert_eq!(
            proc.cwd.as_deref(),
            Some(std::path::Path::new("/work/here"))
        );
        assert_eq!(proc.env.get("KEPT").map(String::as_str), Some("1"));
        assert_eq!(proc.env_remove, ["TAKEN_AWAY"]);
    }

    #[test]
    fn the_totals_are_read_under_either_name_and_cost_in_cents() {
        let t = totals_of(&json!({ "promptTokens": 10, "outputTokens": 4, "cost": 0.0151 }));
        assert_eq!((t.prompt, t.output, t.cents), (10, 4, 2));
        let t = totals_of(&json!({ "inputTokens": 7, "outputTokens": 1 }));
        assert_eq!((t.prompt, t.output, t.cents), (7, 1, 0));
        assert_eq!(totals_of(&Value::Null).prompt, 0);
    }

    #[test]
    fn a_refusal_says_itself() {
        assert_eq!(
            HostRefusal::NotStarted("muse not found on PATH".into()).to_string(),
            "the host could not be started: muse not found on PATH"
        );
        assert_eq!(
            HostRefusal::Refused {
                code: Some(-32030),
                message: "rejected".into()
            }
            .to_string(),
            "the host refused (-32030): rejected"
        );
        assert_eq!(
            HostRefusal::Refused {
                code: None,
                message: "rejected".into()
            }
            .to_string(),
            "the host refused: rejected"
        );
        assert_eq!(
            HostRefusal::Ended("exit 3".into()).to_string(),
            "the host ended before answering: exit 3"
        );
        assert_eq!(
            HostRefusal::TimedOut(8).to_string(),
            "the host did not answer within 8 s"
        );
    }

    // ---- the state machine, driven in-process ----

    /// A session mid-handshake over a channel the test reads, with no
    /// process behind it.
    fn rig(
        model: Option<&str>,
        effort: Option<Effort>,
        prompt: Option<&str>,
        resume: Option<&str>,
    ) -> (Shared, MspState, mpsc::Receiver<OutMsg>) {
        let (out_tx, out_rx) = mpsc::channel(16);
        let model = model.map(str::to_string);
        let shared =
            Shared::new(out_tx, "muse-code", model.clone(), "/work/here").at_effort(effort);
        let state = MspState {
            beginning: Arc::new(std::sync::Mutex::new(Beginning {
                exists: false,
                first: prompt.map(|p| (ID_FIRST_TURN, PromptInput::from(p))),
            })),
            model,
            effort,
            effort_word: Arc::default(),
            rows: Vec::new(),
            resume: resume.map(str::to_string),
            cwd: "/work/here".into(),
            mcp_servers: json!({ "bisa": { "transport": "stdio", "command": "bisa", "args": [], "env": {} } }),
            turn_ids: Arc::default(),
            pending: Arc::default(),
            items: HashMap::new(),
            children: HashMap::new(),
            model_seen: None,
            totals: Totals::default(),
            usage_heard: false,
            observed: Some(Arc::default()),
        };
        (shared, state, out_rx)
    }

    fn drain(out_rx: &mut mpsc::Receiver<OutMsg>) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(msg) = out_rx.try_recv() {
            if let OutMsg::Json(v) = msg {
                out.push(v);
            }
        }
        out
    }

    fn init_result() -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "result": { "serverInfo": { "name": "muse-session-server", "version": "1.4.10" }, "museHome": "/home/me/.local/share/muse", "schema": { "version": 1, "fingerprint": SCHEMA_FINGERPRINT }, "grantedCapabilities": [], "experimentalApi": true, "sessionDurability": "durable" } })
    }

    fn models_result() -> Value {
        json!({ "jsonrpc": "2.0", "id": 2, "result": { "models": [row("small", true, &["low", "high"]), row("large", false, &["low", "high", "max"])] } })
    }

    fn session_result(sid: &str, model: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 3, "result": { "session": { "sessionId": sid, "status": "idle", "turnCount": 0, "workspaceRoot": "/work/here", "providerId": "muse", "modelId": model }, "viewCursor": "v4:x:3" } })
    }

    fn heard(events: &mut BoxEventStream) -> Vec<SessionEvent> {
        use futures::{FutureExt, StreamExt};
        let mut out = Vec::new();
        while let Some(Some(ev)) = events.next().now_or_never() {
            out.push(ev);
        }
        out
    }

    #[tokio::test]
    async fn the_handshake_lists_the_models_starts_the_session_on_the_model_under_the_mode_and_turns_at_a_fitted_level(
    ) {
        let (shared, mut state, mut out_rx) =
            rig(Some("large"), Some(Effort::Xhigh), Some("hello"), None);
        let mut events = shared.broadcaster.subscribe();
        assert!(matches!(
            map_msp(&shared, &mut state, init_result()),
            Drive::Continue
        ));
        let sent = drain(&mut out_rx);
        assert_eq!(
            sent[0],
            json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} })
        );
        assert_eq!(sent[1]["method"], "model/list");
        assert!(matches!(
            map_msp(&shared, &mut state, models_result()),
            Drive::Continue
        ));
        let sent = drain(&mut out_rx);
        assert_eq!(sent.len(), 1);
        let start = &sent[0];
        assert_eq!(start["id"], 3);
        assert_eq!(start["method"], "session/start");
        assert_eq!(start["params"]["workspaceRoot"], "/work/here");
        assert_eq!(start["params"]["modelId"], "large");
        assert_eq!(start["params"]["approvalMode"], "promptUnmatched");
        assert_eq!(
            start["params"]["config"]["mcpServers"]["bisa"]["transport"],
            "stdio"
        );
        assert_eq!(start["params"]["commandId"].as_str().unwrap().len(), 36);
        assert_eq!(
            state.effort_word.locked().as_deref(),
            Some("high"),
            "xhigh held to large's levels"
        );
        assert_eq!(shared.phase(), Phase::Starting);
        // The host says the session started, then answers.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "session/started", "params": { "session": { "sessionId": "s-1" } } }),
        );
        assert!(matches!(
            map_msp(&shared, &mut state, session_result("s-1", "large")),
            Drive::Continue
        ));
        assert_eq!(shared.native_id().as_deref(), Some("s-1"));
        assert_eq!(shared.phase(), Phase::Turn);
        let sent = drain(&mut out_rx);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["method"], "turn/start");
        assert_eq!(sent[0]["id"], ID_FIRST_TURN);
        assert_eq!(sent[0]["params"]["input"][0]["text"], "hello");
        assert_eq!(sent[0]["params"]["reasoningEffort"], "high");
        assert_eq!(sent[0]["params"]["ifBusy"], "queue");
        let seen = heard(&mut events);
        assert!(seen.iter().any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::ModelChanged { model }) if model == "large")), "{seen:?}");
        // The ack, the turn, its words, its end.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 5, "result": { "status": "accepted", "turnId": "t-1", "disposition": "started", "startedNewTurn": true } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "turn/started", "params": { "sessionId": "s-1", "turnId": "t-1" } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "item/started", "params": { "sessionId": "s-1", "item": { "itemId": "i-1", "kind": "agentMessage", "status": "inProgress", "text": "" } } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "item/delta", "params": { "sessionId": "s-1", "itemId": "i-1", "field": "text", "delta": "All 214 tests pass" } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "item/completed", "params": { "sessionId": "s-1", "item": { "itemId": "i-1", "kind": "agentMessage", "status": "completed", "text": "All 214 tests pass" } } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "turn/completed", "params": { "sessionId": "s-1", "turnId": "t-1", "terminal": "completed", "usage": { "inputTokens": 48210, "outputTokens": 1211 } } }),
        );
        assert_eq!(shared.phase(), Phase::Idle);
        let seen = heard(&mut events);
        let texts: Vec<&str> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Progress(ProgressEvent::TextDelta { text }) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            ["All 214 tests pass"],
            "the streamed text once, never again at the item's end: {seen:?}"
        );
        assert!(seen
            .iter()
            .any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::TurnStarted))));
        assert!(seen
            .iter()
            .any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::TurnEnded))));
        assert!(
            seen.iter().any(|e| matches!(
                e,
                SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: Outcome::Completed,
                    is_terminal: false
                })
            )),
            "{seen:?}"
        );
        let cost = shared.snapshot().cost;
        assert_eq!(
            (cost.input_tokens, cost.output_tokens),
            (48210, 1211),
            "the turn's usage, no tokenUsage having come"
        );
        assert!(state.items.is_empty(), "a turn that ends forgets its items");
        let token = shared.resume_token("muse-code").unwrap();
        assert_eq!(token.native_id, "s-1");
        assert_eq!(token.model.as_deref(), Some("large"));
        assert_eq!(token.effort, Some(Effort::Xhigh));
    }

    #[tokio::test]
    async fn a_model_the_host_does_not_list_ends_the_launch_typed_before_any_session_and_an_unlisted_catalog_runs_on(
    ) {
        let (shared, mut state, mut out_rx) =
            rig(Some("huge"), Some(Effort::High), Some("hello"), None);
        map_msp(&shared, &mut state, init_result());
        drain(&mut out_rx);
        match map_msp(&shared, &mut state, models_result()) {
            Drive::End(Outcome::ModelUnavailable { model, reason, .. }) => {
                assert_eq!(model, "huge");
                assert_eq!(reason, MODEL_NOT_LISTED);
            }
            other => panic!("{other:?}"),
        }
        assert!(drain(&mut out_rx).is_empty(), "no session was started");
        assert!(!state.beginning.locked().exists);
        // A host that keeps its list from this client still starts the
        // session, the effort held to every level.
        let (shared, mut state, mut out_rx) = rig(Some("huge"), Some(Effort::Max), None, None);
        map_msp(&shared, &mut state, init_result());
        drain(&mut out_rx);
        assert!(matches!(
            map_msp(
                &shared,
                &mut state,
                json!({ "jsonrpc": "2.0", "id": 2, "error": { "code": -32010, "message": "capabilityRequired" } })
            ),
            Drive::Continue
        ));
        let sent = drain(&mut out_rx);
        assert_eq!(sent[0]["method"], "session/start");
        assert_eq!(sent[0]["params"]["modelId"], "huge");
        // An empty list says nothing of the model either.
        let (shared, mut state, mut out_rx) = rig(Some("huge"), None, None, None);
        map_msp(&shared, &mut state, init_result());
        drain(&mut out_rx);
        assert!(matches!(
            map_msp(
                &shared,
                &mut state,
                json!({ "jsonrpc": "2.0", "id": 2, "result": { "models": [] } })
            ),
            Drive::Continue
        ));
        assert_eq!(drain(&mut out_rx)[0]["method"], "session/start");
        assert_eq!(
            state.effort_word.locked().clone(),
            None,
            "no effort asked, none sent"
        );
    }

    #[tokio::test]
    async fn the_handshakes_refusals_end_the_launch_in_words() {
        let (shared, mut state, _out_rx) = rig(None, None, None, None);
        match map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32600, "message": "bad client" } }),
        ) {
            Drive::End(Outcome::Failed { error }) => {
                assert_eq!(error, "handshake failed: bad client")
            }
            other => panic!("{other:?}"),
        }
        for (code, word, resume) in [
            (-32021, "another muse host", None),
            (-32029, "untrusted", None),
            (-32020, "no longer holds", Some("s-9")),
            (-32030, "session/start failed: session_id_conflict", None),
            (-32030, "session/resume failed: nope", Some("s-9")),
        ] {
            let (shared, mut state, _out_rx) = rig(None, None, None, resume);
            let message = if code == -32030 {
                word.rsplit(": ").next().unwrap()
            } else {
                "x"
            };
            match map_msp(
                &shared,
                &mut state,
                json!({ "jsonrpc": "2.0", "id": 3, "error": { "code": code, "message": message } }),
            ) {
                Drive::End(Outcome::Failed { error }) => {
                    assert!(error.contains(word), "{code}: {error}")
                }
                other => panic!("{code}: {other:?}"),
            }
        }
        let (shared, mut state, _out_rx) = rig(Some("large"), None, None, Some("s-9"));
        match map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 4, "error": { "code": -32030, "message": "unknown model" } }),
        ) {
            Drive::End(Outcome::ModelUnavailable { reason, .. }) => {
                assert_eq!(reason, "the host refused the model: unknown model")
            }
            other => panic!("{other:?}"),
        }
        // A fingerprint that moved is said, never refused; an ephemeral
        // host has nothing to resume.
        let (shared, mut state, mut out_rx) = rig(None, None, None, None);
        let mut moved = init_result();
        moved["result"]["schema"]["fingerprint"] = json!("sha256:other");
        assert!(matches!(
            map_msp(&shared, &mut state, moved),
            Drive::Continue
        ));
        assert_eq!(drain(&mut out_rx).len(), 2);
        let (shared, mut state, _out_rx) = rig(None, None, None, Some("s-9"));
        let mut ephemeral = init_result();
        ephemeral["result"]["sessionDurability"] = json!("ephemeral");
        match map_msp(&shared, &mut state, ephemeral) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("ephemeral"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        // An error on a request nobody waits on is kept, and the session goes on.
        let (shared, mut state, _out_rx) = rig(None, None, None, None);
        let mut events = shared.broadcaster.subscribe();
        assert!(matches!(
            map_msp(
                &shared,
                &mut state,
                json!({ "jsonrpc": "2.0", "id": 77, "error": { "code": -32601, "message": "methodNotFound" } })
            ),
            Drive::Continue
        ));
        assert!(matches!(
            heard(&mut events).as_slice(),
            [SessionEvent::Raw(_)]
        ));
    }

    #[tokio::test]
    async fn a_resume_reloads_the_session_says_its_past_once_and_sets_the_model_back() {
        let (shared, mut state, mut out_rx) =
            rig(Some("large"), Some(Effort::Low), None, Some("s-9"));
        let mut events = shared.broadcaster.subscribe();
        map_msp(&shared, &mut state, init_result());
        drain(&mut out_rx);
        map_msp(&shared, &mut state, models_result());
        let sent = drain(&mut out_rx);
        assert_eq!(sent[0]["method"], "session/resume");
        assert_eq!(sent[0]["params"]["sessionId"], "s-9");
        assert_eq!(sent[0]["params"]["excludeItems"], true);
        assert!(sent[0]["params"].get("workspaceRoot").is_none());
        // Loaded on another model, with a history and a last turn.
        let mut result = session_result("s-9", "small");
        result["result"]["history"] = json!({ "mode": "inline", "items": [{ "itemId": "old", "kind": "agentMessage", "text": "earlier words" }] });
        result["result"]["lastTurn"] =
            json!({ "terminal": "failed", "turnId": "t-0", "reason": "crash_inferred" });
        result["result"]["pendingRequests"] =
            json!([{ "kind": "approval", "viewCursor": "v4:x:9" }]);
        assert!(matches!(
            map_msp(&shared, &mut state, result),
            Drive::Continue
        ));
        let sent = drain(&mut out_rx);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["method"], "session/setModel");
        assert_eq!(sent[0]["params"]["model"]["modelId"], "large");
        assert_eq!(sent[0]["params"]["sessionId"], "s-9");
        let seen = heard(&mut events);
        assert!(
            seen.iter()
                .filter(|e| matches!(e, SessionEvent::Raw(_)))
                .count()
                >= 2,
            "the past and the last turn, as records: {seen:?}"
        );
        assert!(
            !seen
                .iter()
                .any(|e| matches!(e, SessionEvent::Progress(ProgressEvent::TextDelta { .. }))),
            "never said again as if now"
        );
        assert!(
            !state.beginning.locked().exists,
            "not begun until the model is back"
        );
        assert!(matches!(
            map_msp(
                &shared,
                &mut state,
                json!({ "jsonrpc": "2.0", "id": 4, "result": { "commandId": "c", "status": "accepted" } })
            ),
            Drive::Continue
        ));
        assert!(state.beginning.locked().exists);
        assert_eq!(shared.phase(), Phase::Idle);
        let seen = heard(&mut events);
        let models: Vec<&str> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Progress(ProgressEvent::ModelChanged { model }) => {
                    Some(model.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(models, ["large"], "the model moved once, to the one asked");
        // A resume on the right model begins at once; one whose answer names
        // no id keeps the id it asked for.
        let (shared, mut state, mut out_rx) = rig(Some("small"), None, None, Some("s-9"));
        map_msp(&shared, &mut state, init_result());
        map_msp(&shared, &mut state, models_result());
        drain(&mut out_rx);
        let mut result = session_result("s-9", "small");
        result["result"]["session"]
            .as_object_mut()
            .unwrap()
            .remove("sessionId");
        assert!(matches!(
            map_msp(&shared, &mut state, result),
            Drive::Continue
        ));
        assert_eq!(shared.native_id().as_deref(), Some("s-9"));
        assert!(state.beginning.locked().exists);
        assert!(
            drain(&mut out_rx).is_empty(),
            "no prompt kept: nothing goes out"
        );
        // A token that names no model takes the session as the host loaded
        // it, and the kept prompt goes out at once.
        let (shared, mut state, mut out_rx) = rig(None, None, Some("hello"), Some("s-9"));
        map_msp(&shared, &mut state, init_result());
        map_msp(&shared, &mut state, models_result());
        drain(&mut out_rx);
        assert!(matches!(
            map_msp(&shared, &mut state, session_result("s-9", "small")),
            Drive::Continue
        ));
        let sent = drain(&mut out_rx);
        assert_eq!(sent.len(), 1, "no model to set: {sent:?}");
        assert_eq!(sent[0]["method"], "turn/start");
        // A session the host answers with no id, when none was asked for,
        // has nothing to address a turn to: the prompt waits.
        let (shared, mut state, mut out_rx) = rig(None, None, Some("hello"), None);
        map_msp(&shared, &mut state, init_result());
        map_msp(&shared, &mut state, models_result());
        drain(&mut out_rx);
        let mut nameless = session_result("s-1", "small");
        nameless["result"]["session"]
            .as_object_mut()
            .unwrap()
            .remove("sessionId");
        assert!(matches!(
            map_msp(&shared, &mut state, nameless),
            Drive::Continue
        ));
        assert!(shared.native_id().is_none());
        assert!(state.beginning.locked().exists);
        assert!(
            drain(&mut out_rx).is_empty(),
            "nothing to address a turn to"
        );
    }

    #[tokio::test]
    async fn an_approval_is_acknowledged_kept_and_told_once_and_answered_with_the_once_choice() {
        let (shared, mut state, mut out_rx) = rig(None, None, None, None);
        let mut events = shared.broadcaster.subscribe();
        shared.set_native_id("s-1");
        let params = json!({ "sessionId": "s-1", "turnId": "t-1", "itemId": "i-2", "toolCallId": "call_9f21", "toolName": "bash", "rawArgs": "{\"command\":\"rm -rf build\"}", "currentRequirementId": { "approvalId": "a-1", "sourceIndex": 0 }, "approvalId": "a-1", "availableChoices": [
            { "choiceId": "allow_session", "decision": "approvedForSession", "scope": "session", "label": "Allow for this session" },
            { "choiceId": "allow_once", "decision": "approved", "scope": "once", "label": "Allow once" },
            { "choiceId": "abort", "decision": "abort", "scope": "once", "label": "Abort", "acceptsFeedback": true }
        ] });
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "approval/requested", "params": params }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "approval/request", "params": params }),
        );
        assert_eq!(shared.phase(), Phase::AwaitingInput);
        let sent = drain(&mut out_rx);
        assert_eq!(
            sent,
            [json!({ "jsonrpc": "2.0", "id": 1, "result": {} })],
            "the request acknowledged, the notification not"
        );
        let seen = heard(&mut events);
        let asked: Vec<&InputRequest> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    Some(request)
                }
                _ => None,
            })
            .collect();
        assert_eq!(asked.len(), 1, "told once for the two frames: {seen:?}");
        assert_eq!(asked[0].id, "a-1");
        match &asked[0].kind {
            bisa_harness::InputKind::Permission {
                tool_name,
                tier,
                args_summary,
                input,
            } => {
                assert_eq!(tool_name, "bash");
                assert_eq!(*tier, ToolTier::Exec);
                assert_eq!(input, &json!({ "command": "rm -rf build" }));
                assert!(args_summary.contains("rm -rf build"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            shared.snapshot().activity.as_deref(),
            Some("asked to run bash")
        );
        // The facade answers through the same pending set.
        let session = MspSession {
            adapter_id: "muse-code".into(),
            shared: shared.clone(),
            next_id: AtomicU64::new(FACADE_IDS_FROM),
            turn_ids: Arc::clone(&state.turn_ids),
            pending: Arc::clone(&state.pending),
            beginning: Arc::clone(&state.beginning),
            effort_word: Arc::default(),
        };
        session.answer("a-1", InputAnswer::ALLOW).await.unwrap();
        let sent = drain(&mut out_rx);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["method"], "approval/decide");
        assert_eq!(sent[0]["params"]["approvalId"], "a-1");
        assert_eq!(sent[0]["params"]["choiceId"], "allow_once");
        assert_eq!(
            sent[0]["params"]["requirementId"],
            json!({ "approvalId": "a-1", "sourceIndex": 0 })
        );
        assert!(sent[0]["params"].get("feedback").is_none());
        assert_eq!(shared.phase(), Phase::Turn);
        assert!(heard(&mut events).iter().any(|e| matches!(e, SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id }) if id == "a-1")));
        assert!(
            matches!(
                session.answer("a-1", InputAnswer::ALLOW).await,
                Err(HarnessError::Protocol(_))
            ),
            "answered once"
        );
        // A deny takes the abort choice with the reason as feedback.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "approval/request", "params": params }),
        );
        drain(&mut out_rx);
        session
            .answer(
                "a-1",
                InputAnswer::Deny {
                    reason: "not in this repo".into(),
                },
            )
            .await
            .unwrap();
        let sent = drain(&mut out_rx);
        assert_eq!(sent[0]["params"]["choiceId"], "abort");
        assert_eq!(sent[0]["params"]["feedback"], "not in this repo");
        // A host that offers no allowing choice is said.
        let mut only_session = params.clone();
        only_session["approvalId"] = json!("a-2");
        only_session["availableChoices"] =
            json!([{ "choiceId": "allow_session", "decision": "approvedForSession" }]);
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "approval/requested", "params": only_session }),
        );
        match session.answer("a-2", InputAnswer::ALLOW).await {
            Err(HarnessError::Protocol(words)) => {
                assert!(words.contains("no choice that allows"), "{words}")
            }
            other => panic!("{other:?}"),
        }
        // A frame with no approval id is a record and nothing more.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "approval/requested", "params": { "sessionId": "s-1" } }),
        );
        assert!(matches!(
            heard(&mut events).last(),
            Some(SessionEvent::Raw(_))
        ));
    }

    #[tokio::test]
    async fn a_question_is_asked_with_its_labels_and_answered_by_label_or_as_free_text() {
        let (shared, mut state, mut out_rx) = rig(None, None, None, None);
        let mut events = shared.broadcaster.subscribe();
        shared.set_native_id("s-1");
        let params = json!({ "sessionId": "s-1", "turnId": "t-1", "userInputId": "u-1", "questions": [
            { "id": "q-1", "header": "Branch", "question": "Which branch?", "options": [{ "label": "main" }, { "label": "release" }], "selection": "single" },
            { "id": "q-2", "header": "More", "question": "Anything else?", "options": [] }
        ] });
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 9, "method": "userInput/request", "params": params }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": params }),
        );
        assert_eq!(
            drain(&mut out_rx),
            [json!({ "jsonrpc": "2.0", "id": 9, "result": {} })]
        );
        assert_eq!(shared.phase(), Phase::AwaitingInput);
        let seen = heard(&mut events);
        let asked: Vec<&InputRequest> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    Some(request)
                }
                _ => None,
            })
            .collect();
        assert_eq!(asked.len(), 1);
        match &asked[0].kind {
            bisa_harness::InputKind::Question { text, options } => {
                assert_eq!(text, "Which branch?");
                assert_eq!(options, &["main", "release"]);
            }
            other => panic!("{other:?}"),
        }
        let session = MspSession {
            adapter_id: "muse-code".into(),
            shared: shared.clone(),
            next_id: AtomicU64::new(FACADE_IDS_FROM),
            turn_ids: Arc::clone(&state.turn_ids),
            pending: Arc::clone(&state.pending),
            beginning: Arc::clone(&state.beginning),
            effort_word: Arc::default(),
        };
        session
            .answer(
                "u-1",
                InputAnswer::Text {
                    text: "release".into(),
                },
            )
            .await
            .unwrap();
        let sent = drain(&mut out_rx);
        assert_eq!(sent[0]["method"], "userInput/answer");
        assert_eq!(sent[0]["params"]["userInputId"], "u-1");
        assert_eq!(
            sent[0]["params"]["answers"],
            json!([{ "questionId": "q-1", "selectedLabel": "release" }])
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": params }),
        );
        session
            .answer(
                "u-1",
                InputAnswer::Text {
                    text: "the one with the fix".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            drain(&mut out_rx)[0]["params"]["answers"],
            json!([{ "questionId": "q-1", "freeText": "the one with the fix" }])
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": params }),
        );
        session
            .answer(
                "u-1",
                InputAnswer::Deny {
                    reason: "skip it".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            drain(&mut out_rx)[0]["params"]["answers"][0]["freeText"],
            "skip it"
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": params }),
        );
        session.answer("u-1", InputAnswer::ALLOW).await.unwrap();
        assert_eq!(
            drain(&mut out_rx)[0]["params"]["answers"][0]["freeText"],
            ""
        );
        // A question with no words of its own still asks.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": { "sessionId": "s-1", "userInputId": "u-2", "questions": [{ "id": "q-9", "header": "Choose" }] } }),
        );
        let seen = heard(&mut events);
        let last = seen
            .iter()
            .rev()
            .find_map(|e| match e {
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    Some(request)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(last.summary(), "Choose");
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": { "sessionId": "s-1", "userInputId": "u-3" } }),
        );
        let seen = heard(&mut events);
        let last = seen
            .iter()
            .rev()
            .find_map(|e| match e {
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    Some(request)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(last.summary(), "The agent asks for your input.");
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "userInput/requested", "params": { "sessionId": "s-1" } }),
        );
        assert!(matches!(
            heard(&mut events).last(),
            Some(SessionEvent::Raw(_))
        ));
    }

    #[tokio::test]
    async fn tools_reasoning_searches_and_sub_agents_are_told_and_a_sub_agents_items_nest_under_it()
    {
        let (shared, mut state, _out_rx) = rig(None, None, None, None);
        let mut events = shared.broadcaster.subscribe();
        shared.set_native_id("s-1");
        let frame = |method: &str, params: Value| json!({ "jsonrpc": "2.0", "method": method, "params": params });
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-r", "kind": "reasoning", "status": "inProgress" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/delta",
                json!({ "sessionId": "s-1", "itemId": "i-r", "field": "summary.0", "delta": "thinking…" }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-r", "kind": "reasoning", "status": "completed", "text": "thinking… done" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-r2", "kind": "reasoning", "status": "inProgress" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-r2", "kind": "reasoning", "status": "completed", "text": "a thought said whole" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-t", "kind": "toolCall", "status": "inProgress", "tool": "write_file", "callId": "call_1", "args": "{\"path\":\"Cargo.toml\"}" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-t", "kind": "toolCall", "status": "failed", "tool": "write_file", "callId": "call_1", "failureKind": "denied" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-w", "kind": "webSearch", "status": "inProgress", "query": "rust uuid v7" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-w", "kind": "webSearch", "status": "completed" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-s", "kind": "subagent", "status": "inProgress", "subagentId": "sub-1", "role": "explorer", "objective": "map the crate", "childSessionId": "s-child" } }),
            ),
        );
        // The child's own items, on its session.
        map_msp(
            &shared,
            &mut state,
            frame(
                "turn/started",
                json!({ "sessionId": "s-child", "turnId": "t-c" }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-child", "item": { "itemId": "i-cm", "kind": "agentMessage", "status": "inProgress" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-child", "item": { "itemId": "i-cm", "kind": "agentMessage", "status": "completed", "text": "found it" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "turn/completed",
                json!({ "sessionId": "s-child", "turnId": "t-c", "terminal": "completed" }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-s", "kind": "subagent", "status": "completed", "subagentId": "sub-1", "childSessionId": "s-child" } }),
            ),
        );
        // Kinds the protocol may grow, a delta nobody started, an end nobody started.
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/started",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-h", "kind": "hookRun", "status": "inProgress" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/delta",
                json!({ "sessionId": "s-1", "itemId": "i-x", "field": "text", "delta": "?" }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/completed",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-h", "kind": "hookRun", "status": "completed" } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                "item/updated",
                json!({ "sessionId": "s-1", "item": { "itemId": "i-h" } }),
            ),
        );
        assert_ne!(
            shared.phase(),
            Phase::Turn,
            "a child's turn never moves the parent's phase"
        );
        let seen = heard(&mut events);
        let progress: Vec<&ProgressEvent> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Progress(p) => Some(p),
                _ => None,
            })
            .collect();
        assert!(progress
            .iter()
            .any(|p| matches!(p, ProgressEvent::ThinkingDelta { text } if text == "thinking…")));
        assert!(
            !progress.iter().any(
                |p| matches!(p, ProgressEvent::ThinkingDelta { text } if text == "thinking… done")
            ),
            "streamed reasoning is not repeated at its end"
        );
        assert!(
            progress.iter().any(
                |p| matches!(p, ProgressEvent::ThinkingDelta { text } if text == "a thought said whole")
            ),
            "unstreamed reasoning is said whole at its end"
        );
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::ToolStarted { name, tier: ToolTier::Write, id: Some(id), args_summary } if name == "write_file" && id == "call_1" && args_summary.contains("Cargo.toml"))), "{progress:?}");
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::ToolEnded { name, ok: false, id: Some(id) } if name == "write_file" && id == "call_1")));
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::ToolStarted { name, tier: ToolTier::Read, args_summary, .. } if name == "web_search" && args_summary == "rust uuid v7")));
        assert!(progress.iter().any(
            |p| matches!(p, ProgressEvent::ToolEnded { name, ok: true, .. } if name == "web_search")
        ));
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::SubagentStarted { id, name, description } if id.0 == "sub-1" && name == "explorer" && description == "map the crate")));
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::Nested { parent, event } if parent.0 == "sub-1" && matches!(**event, ProgressEvent::TurnStarted))), "{progress:?}");
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::Nested { parent, event } if parent.0 == "sub-1" && matches!(&**event, ProgressEvent::TextDelta { text } if text == "found it"))), "an unstreamed message is said whole, under its sub-agent: {progress:?}");
        assert!(progress.iter().any(|p| matches!(p, ProgressEvent::Nested { parent, event } if parent.0 == "sub-1" && matches!(**event, ProgressEvent::TurnEnded))));
        assert!(progress.iter().any(
            |p| matches!(p, ProgressEvent::SubagentEnded { id, ok: true } if id.0 == "sub-1")
        ));
        assert!(
            state.children.is_empty(),
            "the child is forgotten with its sub-agent"
        );
        assert_eq!(
            seen.iter()
                .filter(|e| matches!(e, SessionEvent::Raw(_)))
                .count(),
            4,
            "the hook run's start and end, the stray delta, the update: {seen:?}"
        );
        // Past the bound, an item is not remembered — and its end is a record.
        for n in 0..MAX_KNOWN_ITEMS + 1 {
            map_msp(
                &shared,
                &mut state,
                frame(
                    "item/started",
                    json!({ "sessionId": "s-1", "item": { "itemId": format!("i-{n}"), "kind": "agentMessage", "status": "inProgress" } }),
                ),
            );
        }
        assert_eq!(state.items.len(), MAX_KNOWN_ITEMS);
    }

    #[tokio::test]
    async fn token_usage_is_counted_once_from_the_totals_and_a_model_that_moves_is_said() {
        let (shared, mut state, _out_rx) = rig(None, None, None, None);
        let mut events = shared.broadcaster.subscribe();
        shared.set_native_id("s-1");
        let frame = |params: Value| json!({ "jsonrpc": "2.0", "method": "session/tokenUsage", "params": params });
        map_msp(
            &shared,
            &mut state,
            frame(
                json!({ "sessionId": "s-1", "turnId": "t-1", "modelId": "small", "usage": { "promptTokens": 100, "outputTokens": 10, "cost": 0.01 }, "cumulative": { "promptTokens": 100, "outputTokens": 10, "cost": 0.01 } }),
            ),
        );
        map_msp(
            &shared,
            &mut state,
            frame(
                json!({ "sessionId": "s-1", "turnId": "t-1", "modelId": "small", "usage": { "promptTokens": 50, "outputTokens": 5, "cost": 0.005 }, "cumulative": { "promptTokens": 150, "outputTokens": 15, "cost": 0.015 } }),
            ),
        );
        // The same totals again: nothing new.
        map_msp(
            &shared,
            &mut state,
            frame(
                json!({ "sessionId": "s-1", "cumulative": { "promptTokens": 150, "outputTokens": 15, "cost": 0.015 } }),
            ),
        );
        // And a turn's end does not count the turn again.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "turn/completed", "params": { "sessionId": "s-1", "terminal": "completed", "usage": { "inputTokens": 150, "outputTokens": 15 } } }),
        );
        let cost = shared.snapshot().cost;
        assert_eq!(
            (cost.input_tokens, cost.output_tokens, cost.usd_cents),
            (150, 15, 2)
        );
        let seen = heard(&mut events);
        let deltas: Vec<(u64, u64, u64)> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens,
                    output_tokens,
                    usd_cents,
                }) => Some((*input_tokens, *output_tokens, *usd_cents)),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, [(100, 10, 1), (50, 5, 1)]);
        let models: Vec<&str> = seen
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Progress(ProgressEvent::ModelChanged { model }) => {
                    Some(model.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(models, ["small"], "said once");
        // Without totals, the usage itself is the delta; the next turn counts its end again.
        map_msp(
            &shared,
            &mut state,
            frame(json!({ "sessionId": "s-1", "usage": { "inputTokens": 7, "outputTokens": 1 } })),
        );
        assert_eq!(shared.snapshot().cost.input_tokens, 157);
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "turn/completed", "params": { "sessionId": "s-1", "terminal": "completed" } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "turn/completed", "params": { "sessionId": "s-1", "terminal": "completed", "usage": { "inputTokens": 3, "outputTokens": 1 } } }),
        );
        assert_eq!(shared.snapshot().cost.input_tokens, 160);
    }

    #[tokio::test]
    async fn a_turn_that_fails_or_is_refused_ends_the_turn_not_the_session_and_the_host_closing_idle_suspends(
    ) {
        let (shared, mut state, mut out_rx) = rig(None, None, None, None);
        let mut events = shared.broadcaster.subscribe();
        shared.set_native_id("s-1");
        state.beginning.locked().exists = true;
        let session = MspSession {
            adapter_id: "muse-code".into(),
            shared: shared.clone(),
            next_id: AtomicU64::new(FACADE_IDS_FROM),
            turn_ids: Arc::clone(&state.turn_ids),
            pending: Arc::clone(&state.pending),
            beginning: Arc::clone(&state.beginning),
            effort_word: Arc::new(std::sync::Mutex::new(Some("low".into()))),
        };
        shared.set_phase(Phase::Idle);
        session.prompt(PromptInput::from("go")).await.unwrap();
        assert_eq!(shared.phase(), Phase::Turn);
        assert!(matches!(
            session.prompt(PromptInput::from("again")).await,
            Err(HarnessError::Busy)
        ));
        let sent = drain(&mut out_rx);
        assert_eq!(sent[0]["method"], "turn/start");
        assert_eq!(sent[0]["params"]["reasoningEffort"], "low");
        let id = sent[0]["id"].as_u64().unwrap();
        // The host refuses the turn: the turn ends, the session idles.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32030, "message": "commandRejected" } }),
        );
        assert_eq!(shared.phase(), Phase::Idle);
        let seen = heard(&mut events);
        assert!(seen.iter().any(|e| matches!(e, SessionEvent::Lifecycle(LifecycleEvent::Ended { outcome: Outcome::Failed { error }, is_terminal: false }) if error == "commandRejected")), "{seen:?}");
        // A steer and a follow-up ride the same frame under their words;
        // a queued ack moves nothing, a started one runs.
        session
            .steer(Steer {
                text: "also".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
        session
            .follow_up(Steer {
                text: "then".into(),
                attachments: vec![],
            })
            .await
            .unwrap();
        let sent = drain(&mut out_rx);
        assert_eq!(sent[0]["params"]["ifBusy"], "steer");
        assert_eq!(sent[1]["params"]["ifBusy"], "queue");
        let queued_id = sent[1]["id"].as_u64().unwrap();
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": queued_id, "result": { "status": "accepted", "disposition": "queued", "startedNewTurn": false } }),
        );
        assert_eq!(shared.phase(), Phase::Idle);
        let started_id = sent[0]["id"].as_u64().unwrap();
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": started_id, "result": { "status": "accepted", "disposition": "started", "startedNewTurn": true } }),
        );
        assert_eq!(shared.phase(), Phase::Turn);
        // A turn that fails on the model walks the plan; the session idles.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "turn/completed", "params": { "sessionId": "s-1", "terminal": "failed", "error": { "kind": "modelError", "message": "overloaded", "retryable": true } } }),
        );
        assert_eq!(shared.phase(), Phase::Idle);
        assert!(heard(&mut events).iter().any(|e| matches!(
            e,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::ModelUnavailable { .. },
                is_terminal: false
            })
        )));
        // The host shutting the session down closes nothing; idling it suspends.
        assert!(matches!(
            map_msp(
                &shared,
                &mut state,
                json!({ "jsonrpc": "2.0", "method": "session/closed", "params": { "sessionId": "s-1", "reason": "hostShutdown" } })
            ),
            Drive::Continue
        ));
        match map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "session/closed", "params": { "sessionId": "s-1", "reason": "idle" } }),
        ) {
            Drive::End(Outcome::Suspended { reason }) => {
                assert!(reason.contains("resumed from its token"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
        // A usage the session hears is kept for the footer; a frame the
        // protocol may grow is a record.
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "usage/changed", "params": { "usage": { "tier": "power" } } }),
        );
        assert!(state.observed.as_ref().unwrap().locked().is_some());
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "method": "session/viewHealthChanged", "params": {} }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 999, "result": {} }),
        );
        let seen = heard(&mut events);
        assert!(
            seen.iter()
                .filter(|e| matches!(e, SessionEvent::Raw(_)))
                .count()
                >= 3,
            "{seen:?}"
        );
        // A session ended takes no more turns; one not yet begun keeps one and refuses a second.
        shared.end(Outcome::Completed);
        assert!(matches!(
            session.prompt(PromptInput::from("late")).await,
            Err(HarnessError::Terminated)
        ));
        assert!(matches!(
            session
                .steer(Steer {
                    text: "late".into(),
                    attachments: vec![]
                })
                .await,
            Err(HarnessError::Terminated)
        ));
        let (shared, state, _out_rx) = rig(None, None, None, None);
        let early = MspSession {
            adapter_id: "muse-code".into(),
            shared,
            next_id: AtomicU64::new(FACADE_IDS_FROM),
            turn_ids: Arc::clone(&state.turn_ids),
            pending: Arc::clone(&state.pending),
            beginning: Arc::clone(&state.beginning),
            effort_word: Arc::default(),
        };
        early.prompt(PromptInput::from("first")).await.unwrap();
        assert!(matches!(
            early.prompt(PromptInput::from("second")).await,
            Err(HarnessError::Busy)
        ));
        assert_eq!(
            state
                .beginning
                .locked()
                .first
                .as_ref()
                .map(|(_, p)| p.text.as_str()),
            Some("first")
        );
        assert!(matches!(
            early.answer("nobody", InputAnswer::ALLOW).await,
            Err(HarnessError::Protocol(_))
        ));
        assert_eq!(early.phase(), Phase::Starting);
        assert_eq!(early.snapshot().revision, early.shared.snapshot().revision);
        assert!(early.resume_token().is_none(), "no id yet, no token");
    }

    #[tokio::test]
    async fn a_queue_the_driver_cannot_take_ends_the_session_saying_which_step() {
        let (out_tx, out_rx) = mpsc::channel(1);
        let shared = Shared::new(out_tx, "muse-code", None, "/work/here");
        shared.try_send(OutMsg::Line("full".into())).unwrap();
        let (_, mut state, _unused) = rig(None, None, Some("hello"), None);
        match map_msp(&shared, &mut state, init_result()) {
            Drive::End(Outcome::Failed { error }) => assert!(
                error.contains("initialized notification was not queued"),
                "{error}"
            ),
            other => panic!("{other:?}"),
        }
        let (_, mut state, _unused) = rig(None, None, Some("hello"), None);
        match map_msp(&shared, &mut state, models_result()) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("session/start was not queued"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        shared.set_native_id("s-1");
        let (_, mut state, _unused) = rig(None, None, Some("hello"), None);
        match map_msp(&shared, &mut state, session_result("s-1", "small")) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("first turn was not queued"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        let (_, mut state, _unused) = rig(Some("large"), None, None, Some("s-1"));
        match map_msp(&shared, &mut state, session_result("s-1", "small")) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("model was not queued"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        // The session step after a model list the host refused, and the
        // first turn after the model was set back, meet the same full queue.
        let (_, mut state, _unused) = rig(None, None, Some("hello"), None);
        match map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 2, "error": { "code": -32010, "message": "capabilityRequired" } }),
        ) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("session/start was not queued"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        let (_, mut state, _unused) = rig(Some("large"), None, Some("hello"), Some("s-1"));
        match map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 4, "result": { "commandId": "c", "status": "accepted" } }),
        ) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("first turn was not queued"), "{error}")
            }
            other => panic!("{other:?}"),
        }
        // An acknowledgement the queue cannot take is said in the log, and
        // the ask is still told.
        let mut events = shared.broadcaster.subscribe();
        let (_, mut state, _unused) = rig(None, None, None, None);
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "approval/request", "params": { "sessionId": "s-1", "approvalId": "a-9", "toolName": "bash", "rawArgs": "{}", "currentRequirementId": { "approvalId": "a-9", "sourceIndex": 0 }, "availableChoices": [] } }),
        );
        map_msp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "userInput/request", "params": { "sessionId": "s-1", "userInputId": "u-9", "questions": [{ "questionId": "q", "question": "Which?" }] } }),
        );
        assert_eq!(
            heard(&mut events)
                .iter()
                .filter(|e| matches!(
                    e,
                    SessionEvent::Lifecycle(LifecycleEvent::InputRequested { .. })
                ))
                .count(),
            2
        );
        drop(out_rx);
        // A queue with room for one takes the `initialized` notification
        // and refuses the model list behind it.
        let (out_tx, out_rx) = mpsc::channel(2);
        let shared = Shared::new(out_tx, "muse-code", None, "/work/here");
        shared.try_send(OutMsg::Line("one".into())).unwrap();
        let (_, mut state, _unused) = rig(None, None, None, None);
        match map_msp(&shared, &mut state, init_result()) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(
                    error.contains("model/list request was not queued"),
                    "{error}"
                )
            }
            other => panic!("{other:?}"),
        }
        drop(out_rx);
    }
}
