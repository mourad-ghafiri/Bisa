//! The Agent Client Protocol — JSON-RPC 2.0 on stdio — and the generic
//! adapter over it.
//!
//! **The protocol is this module; a harness is its facts.** Every ACP
//! session in the tree is opened through one door, [`open`], given an
//! [`AcpCommand`] — whose session it is, the program, the words that put it
//! in protocol mode. [`AcpAdapter`] is the generic caller: one per known
//! target (goose, cursor-agent, `omp acp`, opencode's acp mode, ...), four
//! words each. A harness with an id of its own — GitHub Copilot CLI
//! (`copilot.rs`), Grok Build (`grok.rs`), Gemini CLI (`gemini.rs`) — is
//! another caller: its module
//! says what only that tool knows (its probe, its levels, its models, its
//! terminal form) and no frame is written outside this file.
//!
//! Four methods are enough (`initialize`, `session/new`,
//! `session/prompt` streaming `session/update`, stop reason).
//!
//! Implemented as raw JSON-RPC rather than via the `agent-client-protocol`
//! crate: the adapter needs only a thin, version-tolerant subset, and owning
//! the frames keeps the mapper in one place with no API-churn risk.
//!
//! Model and effort: an agent says what it takes only once a session
//! exists — the `configOptions` of the `session/new` (or `session/load`)
//! result, where the option whose `category` is `model` chooses the model
//! and the one whose `category` is `thought_level` how hard it works
//! (https://agentclientprotocol.com/protocol/session-config-options, read
//! 2026-09-29 and 2026-09-30). Both are set with
//! `session/set_config_option` before the first prompt, **the model first**:
//! the answer to a set is the whole option list again, and the levels a
//! session offers are its model's. So the model is set, its answer is read,
//! and the level asked is held to the levels *that* list offers.
//!
//! A session is never recorded on a model it is not running. A session that
//! chooses its model among ones that do not include the model asked, or
//! that refuses it, ends as *model unavailable* — the launch walks the
//! agent's plan to its next model. A session that offers no model option
//! cannot be told, and runs the agent's own; one that offers no
//! `thought_level` option is sent no effort.
//!
//! One agent still speaks the draft that came before config options: Gemini
//! CLI answers `session/new` and `session/load` with `models` —
//! `availableModels`, each a `modelId` and a `name`, and `currentModelId` —
//! and takes `session/set_model` with a `modelId`, answered empty (its
//! `packages/cli/src/acp/acpSessionManager.ts` and `acpRpcDispatcher.ts`,
//! read 2026-10-05; the protocol's site says the method never stabilised and
//! models now ride config options,
//! https://agentclientprotocol.com/announcements/session-config-options-stabilized).
//! The rule keeps its shape: the session on the model already is told
//! nothing, one that lists it is set, and one that lists others ends as
//! *model unavailable*. An agent that offers both is read by its `model`
//! config option.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bisa_core::{HarnessCaps, ToolTier};
use bisa_harness::proc::{Line, ProcHandle, ProcSpec};
use bisa_harness::{
    BoxEventStream, Effort, HarnessAdapter, HarnessError, HarnessSession, InputAnswer,
    InputRequest, LifecycleEvent, McpMount, McpServerConfig, Outcome, Phase, ProbeResult,
    ProgressEvent, PromptInput, ResumeToken, SessionEvent, SessionSnapshot, SessionSpec, Steer,
};
use tokio::sync::mpsc;

use crate::util::{self, Drive, OutMsg, Shared};
use bisa_core::sync::Locked;

/// Request ids used by the driver's handshake.
const ID_INIT: u64 = 1;
const ID_SESSION: u64 = 2;
const ID_FIRST_PROMPT: u64 = 3;
const ID_EFFORT: u64 = 4;
const ID_MODEL: u64 = 5;

/// The `category` of the config option that sets how hard the agent's model
/// works.
const EFFORT_CATEGORY: &str = "thought_level";

/// The `category` of the config option that chooses the session's model.
const MODEL_CATEGORY: &str = "model";

/// Why a session that chooses its model among others ends.
const MODEL_NOT_OFFERED: &str = "the agent's session does not offer this model";

/// Why a revival asked of an agent that loads no session ends.
const CANNOT_LOAD: &str =
    "the agent cannot load an earlier session: it advertises no `loadSession` capability";

/// How one ACP agent is started: whose session it is, the program, and the
/// words that put it in protocol mode. Everything else a session needs —
/// where it runs, its environment, its MCP servers, the model and the effort
/// — is the [`SessionSpec`]'s, and reaches the agent the protocol's way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCommand {
    /// The adapter the session belongs to: `acp:goose`, `copilot`, `grok`, `gemini`.
    pub adapter_id: String,
    pub program: String,
    pub args: Vec<String>,
}

impl AcpCommand {
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

pub struct AcpAdapter {
    /// Full adapter id, e.g. "acp" or "acp:goose".
    pub adapter_id: String,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
}

impl AcpAdapter {
    pub fn new(
        adapter_id: impl Into<String>,
        label: impl Into<String>,
        program: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            label: label.into(),
            program: program.into(),
            args,
        }
    }

    /// The bare generic adapter (`acp`); target selected via SessionSpec env
    /// is not supported — construct one AcpAdapter per target instead.
    pub fn generic() -> Self {
        Self::new("acp", "ACP agent", "acp-agent", vec![])
    }

    /// How this target is started.
    pub fn command(&self) -> AcpCommand {
        AcpCommand::new(&self.adapter_id, &self.program, &self.args)
    }
}

/// A tool call's `toolCallId`, when a message names one.
fn tool_call_id(update: &serde_json::Value) -> Option<String> {
    update
        .get("toolCallId")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn jsonrpc_request(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn jsonrpc_response(id: &serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// The session's `mcpServers`, in ACP's own shapes
/// (agentclientprotocol.com/protocol/v1/session-setup): a stdio entry is
/// `name`, `command`, `args` and `env` as `{name, value}` pairs — ACP has no
/// `cwd`, so one is dropped with a warning; a remote entry is `type: "http"`
/// or `type: "sse"` with `url` and `headers` as the same pairs.
fn mcp_servers_json(adapter_id: &str, mounts: &[McpMount]) -> serde_json::Value {
    let pairs = |m: &std::collections::BTreeMap<String, String>| {
        m.iter()
            .map(|(k, v)| serde_json::json!({"name": k, "value": v}))
            .collect::<Vec<_>>()
    };
    let list: Vec<serde_json::Value> = mounts
        .iter()
        .map(|m| match &m.config {
            McpServerConfig::Stdio { name, command, args, env, cwd } => {
                if cwd.is_some() {
                    crate::mcp_inject::warn_dropped(adapter_id, name, "cwd");
                }
                serde_json::json!({ "name": name, "command": command, "args": args, "env": pairs(env) })
            }
            McpServerConfig::Http { name, url, headers } => serde_json::json!({
                "type": "http", "name": name, "url": url, "headers": pairs(headers),
            }),
            McpServerConfig::Sse { name, url, headers } => serde_json::json!({
                "type": "sse", "name": name, "url": url, "headers": pairs(headers),
            }),
        })
        .collect();
    serde_json::Value::Array(list)
}

/// The level a config option's value names, whatever its case and its
/// separators: `X-High`, `x_high` and `xhigh` are one level. `None` for a
/// word that is no level.
fn level_of(value: &str) -> Option<Effort> {
    let word: String = value
        .chars()
        .filter(|c| !matches!(*c, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .collect();
    word.parse().ok()
}

/// The values one config option offers, as the agent spells them. An entry
/// is a value, or a group of them.
fn offered_values(option: &serde_json::Value) -> Vec<&str> {
    let entries = option
        .get("options")
        .and_then(|o| o.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    entries
        .iter()
        .flat_map(|entry| {
            let own = entry.get("value").and_then(|v| v.as_str());
            let grouped = entry
                .get("options")
                .and_then(|o| o.as_array())
                .into_iter()
                .flatten()
                .filter_map(|e| e.get("value").and_then(|v| v.as_str()));
            own.into_iter().chain(grouped)
        })
        .collect()
}

/// What to set so the session runs at `asked`: the id of the session's
/// `thought_level` option and the value, in the agent's own spelling, of the
/// level `asked` comes to among the ones the option offers — itself, else
/// the nearest below, else the lowest above. `None` when the session offers
/// no such option, or none of its values is a level.
fn effort_option(config_options: &serde_json::Value, asked: Effort) -> Option<(String, String)> {
    let option = option_of(config_options, EFFORT_CATEGORY)?;
    let id = option.get("id").and_then(|i| i.as_str())?;
    let offered: Vec<(Effort, &str)> = offered_values(option)
        .into_iter()
        .filter_map(|value| level_of(value).map(|level| (level, value)))
        .collect();
    let levels: Vec<Effort> = offered.iter().map(|(level, _)| *level).collect();
    let level = asked.clamp_to(&levels)?;
    let (_, value) = offered.iter().find(|(l, _)| *l == level)?;
    Some((id.to_string(), value.to_string()))
}

/// The first of a session's config options under `category` — the order is
/// the agent's own priority.
fn option_of<'a>(
    config_options: &'a serde_json::Value,
    category: &str,
) -> Option<&'a serde_json::Value> {
    config_options
        .as_array()?
        .iter()
        .find(|o| o.get("category").and_then(|c| c.as_str()) == Some(category))
}

/// What a session's answer says of the model asked.
#[derive(Debug, PartialEq, Eq)]
enum ModelStep {
    /// Nothing to send: the session has no model option to set — it runs the
    /// agent's own and cannot be told — or it is on the model already.
    Nothing,
    /// Set the option of this id to the model.
    Set(String),
    /// Set the model the draft's way — `session/set_model` — which names no
    /// option: the agent listed its `models` and offers no `model` option.
    SetModel,
    /// The session chooses its model among ones that do not include it.
    NotOffered,
}

/// What to do so the session runs `asked`, read off the answer to
/// `session/new` or `session/load`: its `model` config option when it has
/// one, else the draft's `models` list. A model is matched by its id as the
/// agent spells it, exactly: a model is a name, never a level to fit — and
/// a listed model's `name` is a label, not its id.
fn model_step(answer: &serde_json::Value, asked: &str) -> ModelStep {
    let option = answer
        .get("configOptions")
        .and_then(|options| option_of(options, MODEL_CATEGORY));
    if let Some(option) = option {
        if option.get("currentValue").and_then(|v| v.as_str()) == Some(asked) {
            return ModelStep::Nothing;
        }
        let Some(id) = option.get("id").and_then(|i| i.as_str()) else {
            // An option with no id cannot be set, and says nothing of what the
            // session would take.
            return ModelStep::Nothing;
        };
        return if offered_values(option).contains(&asked) {
            ModelStep::Set(id.to_string())
        } else {
            ModelStep::NotOffered
        };
    }
    let Some(listed) = answer
        .pointer("/models/availableModels")
        .and_then(|m| m.as_array())
    else {
        return ModelStep::Nothing;
    };
    if answer
        .pointer("/models/currentModelId")
        .and_then(|v| v.as_str())
        == Some(asked)
    {
        return ModelStep::Nothing;
    }
    if listed
        .iter()
        .any(|m| m.get("modelId").and_then(|v| v.as_str()) == Some(asked))
    {
        ModelStep::SetModel
    } else {
        ModelStep::NotOffered
    }
}

/// The request that sets a session's config option.
fn set_config_option(
    request: u64,
    session_id: &str,
    config_id: &str,
    value: &str,
) -> serde_json::Value {
    jsonrpc_request(
        request,
        "session/set_config_option",
        serde_json::json!({ "sessionId": session_id, "configId": config_id, "value": value }),
    )
}

/// The request that sets a session's model the draft's way: no option, the
/// model alone.
fn set_model(request: u64, session_id: &str, model_id: &str) -> serde_json::Value {
    jsonrpc_request(
        request,
        "session/set_model",
        serde_json::json!({ "sessionId": session_id, "modelId": model_id }),
    )
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
    // The token carries the directory the launch chose. This used to be
    // `current_dir()`, falling back to `/` — the daemon's own placement,
    // which is never the session's. It carries the model and the effort
    // too, set again once the loaded session says what it takes.
    Ok(revival_spec(token))
}

/// Queue one message of the protocol. The outbound queue is bounded, and a
/// message it cannot take is a step lost — nothing would ever answer it — so
/// the session ends saying which, never left starting, or holding a first
/// prompt nobody will run, until a clock somewhere else notices.
fn queued(shared: &Shared, what: &str, message: serde_json::Value) -> Result<(), Outcome> {
    shared
        .out_tx
        .try_send(OutMsg::Json(message))
        .map_err(|e| Outcome::Failed {
            error: format!("the {what} was not queued: {e}"),
        })
}

fn revival_spec(token: &ResumeToken) -> SessionSpec {
    SessionSpec {
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
    }
}

/// Driver-side handshake state.
/// A session's beginning, as the driver and the facade both read it: whether
/// the session exists yet, and the prompt kept for it while it does not.
///
/// The engine launches a session with no prompt and prompts it at once —
/// before the agent has answered `initialize`. That prompt is the session's
/// first: it waits here, and the driver sends it the moment `session/new` (or
/// `session/load`) is answered. One lock for both halves, so a prompt is
/// either kept before the session exists or sent after, never lost between.
#[derive(Default)]
struct Beginning {
    /// The agent answered `session/new` or `session/load`.
    exists: bool,
    /// The first prompt and the request id it goes out under.
    first: Option<(u64, String)>,
}

type SharedBeginning = Arc<std::sync::Mutex<Beginning>>;

/// How many tool calls a session is remembered to have open at once. A turn
/// that ends forgets them all; within one, a call past the bound is read by
/// what its own messages carry.
const MAX_KNOWN_TOOLS: usize = 256;

/// A tool call as the agent has described it so far, kept by its id: every
/// message about a call after the first may name the id and nothing else
/// ("All fields except `toolCallId` are optional in updates" —
/// agentclientprotocol.com/protocol/tool-calls, read 2026-09-30 — and a
/// permission request's `toolCall` is such an update).
#[derive(Debug, Default)]
struct KnownTool {
    /// The first title heard: the name the call started under, and the one
    /// it is asked about and ends under — what the roster opened and the
    /// review bracketed is closed by the same word.
    name: Option<String>,
    kind: Option<String>,
    raw_input: serde_json::Value,
}

impl KnownTool {
    /// Take in what one message says of the call. A name is kept once said;
    /// a kind and an input are the latest said.
    fn hears(&mut self, said: &serde_json::Value) {
        if self.name.is_none() {
            self.name = said
                .get("title")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        if let Some(kind) = said.get("kind").and_then(|v| v.as_str()) {
            self.kind = Some(kind.to_string());
        }
        if let Some(input) = said.get("rawInput").filter(|v| !v.is_null()) {
            self.raw_input = input.clone();
        }
    }

    fn name(&self) -> String {
        self.name.clone().unwrap_or_else(|| "?".to_string())
    }

    fn tier(&self) -> ToolTier {
        acp_kind_tier(self.kind.as_deref())
    }
}

struct AcpState {
    /// The session's beginning, shared with the facade's `prompt`.
    beginning: SharedBeginning,
    /// The model to set as soon as the session says which it offers.
    model: Option<String>,
    /// The effort to set as soon as the session says what it takes.
    effort: Option<Effort>,
    /// The session's config options as `session/new` answered them, kept
    /// while the model is being set: the answer to that set is the list to
    /// fit the effort to, and this one stands in when it carries none.
    options: serde_json::Value,
    /// The session to load when re-attaching — kept until the agent has
    /// answered: the answer to `session/load` carries no id, so the id of a
    /// loaded session is this one.
    load_session: Option<String>,
    /// A `session/load` is in flight: what the agent streams until it
    /// answers is the conversation as it was, replayed — the past, never the
    /// revived session's own words.
    replaying: bool,
    cwd: String,
    mcp_servers: serde_json::Value,
    /// Ids of in-flight prompt requests (first prompt uses ID_FIRST_PROMPT;
    /// facade-issued ones use its atomic counter). Shared with the facade so
    /// `prompt()` can register an id before its request hits the wire.
    prompt_ids: Arc<std::sync::Mutex<Vec<u64>>>,
    /// Permission requests the agent is waiting on: the JSON-RPC id and the
    /// options it offered, keyed by the id's text. Shared with the facade,
    /// whose `answer` sends the response.
    pending: PendingPermissions,
    /// The tool calls under way, by their id — at most [`MAX_KNOWN_TOOLS`],
    /// each forgotten when it ends and all of them when the turn does.
    tools: HashMap<String, KnownTool>,
}

/// The JSON-RPC id and the offered options of every unanswered
/// `session/request_permission`, keyed by the id's text.
type PendingPermissions =
    Arc<std::sync::Mutex<HashMap<String, (serde_json::Value, Vec<serde_json::Value>)>>>;

#[async_trait]
impl HarnessAdapter for AcpAdapter {
    fn id(&self) -> &str {
        &self.adapter_id
    }

    fn display_name(&self) -> &str {
        &self.label
    }

    fn caps(&self) -> HarnessCaps {
        // `session/request_permission` stops before the tool runs and obeys a
        // refusal, so the guard can veto; the reply picks an option, so no
        // input can be rewritten.
        HarnessCaps::MCP_SERVERS
            | HarnessCaps::INPUT_REQUESTS
            | HarnessCaps::RESUME
            | HarnessCaps::TOOL_GUARD
            | HarnessCaps::EFFORT
    }

    /// All six may be asked for: an ACP agent says which levels it takes
    /// only once a session is open, and the level is held to those there.
    fn efforts(&self, model: Option<&str>) -> Vec<Effort> {
        let _ = model;
        Effort::ALL.to_vec()
    }

    async fn probe(&self) -> ProbeResult {
        if which::which(&self.program).is_err() {
            return ProbeResult::unavailable(format!("{} not found on PATH", self.program));
        }
        ProbeResult::available(None)
    }

    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        open(self.command(), &spec, None).await
    }

    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        let spec = revival(&self.adapter_id, token)?;
        open(self.command(), &spec, Some(token.native_id.clone())).await
    }
}

/// The process `command` starts for `spec`: the program and its words, in
/// the session's directory, with the session's environment — what the spec
/// adds, then what it takes away, so a name it removes stays removed.
fn proc_spec(command: &AcpCommand, spec: &SessionSpec) -> ProcSpec {
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

/// What an agent that left with status 0 has done: finished, once its
/// session existed — and failed to start, when it left before that. An
/// installer stub that prints a question and exits, a binary that does not
/// know the protocol word it was given: neither is a completed run.
fn clean_exit(beginning: &SharedBeginning, stderr_tail: &[String]) -> Outcome {
    if beginning.locked().exists {
        return Outcome::Completed;
    }
    let tail = stderr_tail.join(" | ");
    Outcome::Failed {
        error: if tail.is_empty() {
            "the agent ended before its session began".to_string()
        } else {
            format!("the agent ended before its session began; stderr tail: {tail}")
        },
    }
}

/// Open one ACP session: start the agent `command` names, shake hands, make
/// the session (or load `load_session`), set the model and the effort the
/// spec asks for, and hand back the session — the one door every ACP adapter
/// in the tree goes through.
pub async fn open(
    command: AcpCommand,
    spec: &SessionSpec,
    load_session: Option<String>,
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
    let revived = load_session.is_some();
    shared.broadcaster.emit(SessionEvent::Lifecycle(if revived {
        LifecycleEvent::Revived
    } else {
        LifecycleEvent::Started
    }));

    let prompt_ids: Arc<std::sync::Mutex<Vec<u64>>> = Arc::default();
    let pending: PendingPermissions = Arc::default();
    let beginning: SharedBeginning = Arc::new(std::sync::Mutex::new(Beginning {
        exists: false,
        first: (!spec.prompt.is_empty()).then(|| (ID_FIRST_PROMPT, spec.prompt.clone())),
    }));
    let mut state = AcpState {
        beginning: Arc::clone(&beginning),
        model: spec.model.clone(),
        effort: spec.effort,
        options: serde_json::Value::Null,
        load_session,
        replaying: false,
        cwd: spec.cwd.display().to_string(),
        mcp_servers: mcp_servers_json(&command.adapter_id, &spec.mcp_servers),
        prompt_ids: Arc::clone(&prompt_ids),
        pending: Arc::clone(&pending),
        tools: HashMap::new(),
    };

    // Kick off the handshake before the driver starts: the message is
    // queued on the out channel and written first.
    shared
        .send(OutMsg::Json(jsonrpc_request(
            ID_INIT,
            "initialize",
            serde_json::json!({
                "protocolVersion": 1,
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false }
            }),
        )))
        .await?;

    let driver_shared = shared.clone();
    let began = Arc::clone(&beginning);
    tokio::spawn(util::drive_until(
        proc,
        out_rx,
        driver_shared,
        move |shared, line| {
            let value = match line {
                Line::Json(v) => v,
                Line::Text(t) => {
                    tracing::debug!(target: "bisa_adapters::acp", "non-json stdout: {t}");
                    return Drive::Continue;
                }
            };
            map_acp(shared, &mut state, value)
        },
        move |stderr_tail| clean_exit(&began, stderr_tail),
    ));

    Ok(Box::new(AcpSession {
        adapter_id: command.adapter_id,
        shared,
        next_id: AtomicU64::new(100),
        prompt_ids,
        pending,
        beginning,
    }))
}

fn map_acp(shared: &Shared, state: &mut AcpState, value: serde_json::Value) -> Drive {
    // Response to one of our requests?
    if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
        if value.get("method").is_none() {
            if let Some(error) = value.get("error") {
                let msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("json-rpc error");
                if id == ID_INIT || id == ID_SESSION {
                    // The handshake *is* the launch for an ACP agent, so a
                    // model refusal here is a launch failure — it just cannot
                    // travel back through `launch()`'s Result, which already
                    // returned. The engine reads a `ModelUnavailable` before
                    // any progress as exactly that.
                    if let Some(outcome) = shared.model_ctx.outcome(msg) {
                        return Drive::End(outcome);
                    }
                    return Drive::End(Outcome::Failed {
                        error: format!("handshake failed: {msg}"),
                    });
                }
                if id == ID_MODEL {
                    // An agent that listed its models yet has no
                    // `session/set_model` (-32601, method not found) is a
                    // failure to say, not a model to walk past: every model
                    // of the plan would be refused the same way.
                    if error.get("code").and_then(|c| c.as_i64()) == Some(-32601) {
                        return Drive::End(Outcome::Failed {
                            error: format!("the agent cannot set a model: {msg}"),
                        });
                    }
                    // The agent will not run the model asked: the session
                    // would go on with another while its token and the
                    // ledger name this one. It ends here instead, before any
                    // prompt, and the launch walks the plan to its next.
                    return Drive::End(shared.model_ctx.outcome(msg).unwrap_or_else(|| {
                        shared
                            .model_ctx
                            .typed_outcome(format!("the agent refused the model: {msg}"), None)
                    }));
                }
                tracing::warn!(target: "bisa_adapters::acp", "request {id} failed: {msg}");
                let was_prompt = {
                    let mut ids = state.prompt_ids.locked();
                    let found = ids.contains(&id);
                    ids.retain(|p| *p != id);
                    found
                };
                if was_prompt {
                    // Mid-run site: a prompt that died on the model.
                    let outcome =
                        shared
                            .model_ctx
                            .outcome(msg)
                            .unwrap_or_else(|| Outcome::Failed {
                                error: msg.to_string(),
                            });
                    shared.set_phase(Phase::Idle);
                    shared
                        .broadcaster
                        .emit(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                            outcome,
                            is_terminal: false,
                        }));
                }
                return Drive::Continue;
            }
            let result = value
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match id {
                ID_INIT => {
                    // A session is loaded only by an agent that says it can:
                    // "If `loadSession` is `false` or not present, the Agent
                    // does not support loading sessions and Clients MUST NOT
                    // attempt to call `session/load`"
                    // (agentclientprotocol.com/protocol/session-setup, read
                    // 2026-09-30). Said here in words, rather than read off
                    // whatever an agent answers a method it lacks.
                    if state.load_session.is_some() && !loads_sessions(&result) {
                        return Drive::End(Outcome::Failed {
                            error: CANNOT_LOAD.to_string(),
                        });
                    }
                    // Move to session creation (or load on re-attach).
                    let msg = if let Some(session_id) = &state.load_session {
                        state.replaying = true;
                        jsonrpc_request(
                            ID_SESSION,
                            "session/load",
                            serde_json::json!({
                                "sessionId": session_id, "cwd": state.cwd, "mcpServers": state.mcp_servers
                            }),
                        )
                    } else {
                        jsonrpc_request(
                            ID_SESSION,
                            "session/new",
                            serde_json::json!({ "cwd": state.cwd, "mcpServers": state.mcp_servers }),
                        )
                    };
                    let what = if state.load_session.is_some() {
                        "session/load request"
                    } else {
                        "session/new request"
                    };
                    if let Err(end) = queued(shared, what, msg) {
                        return Drive::End(end);
                    }
                    shared.broadcaster.emit(SessionEvent::Raw(value));
                }
                ID_SESSION => {
                    // A session made anew says its id. One that was loaded
                    // is the one that was asked for: the protocol's answer
                    // to `session/load` is empty
                    // (agentclientprotocol.com/protocol/session-setup).
                    state.replaying = false;
                    let loaded = state.load_session.take();
                    let said = result.get("sessionId").and_then(|v| v.as_str());
                    if let Some(sid) = said.or(loaded.as_deref()) {
                        shared.set_native_id(sid);
                    }
                    let options = result
                        .get("configOptions")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    // The model first: the levels a session offers are its
                    // model's, so the effort waits for the model's answer.
                    let Some((model, sid)) = state.model.take().zip(shared.native_id()) else {
                        return match state.begin(shared, &options) {
                            Ok(()) => Drive::Continue,
                            Err(end) => Drive::End(end),
                        };
                    };
                    match model_step(&result, &model) {
                        ModelStep::Nothing => {
                            if let Err(end) = state.begin(shared, &options) {
                                return Drive::End(end);
                            }
                        }
                        ModelStep::NotOffered => {
                            return Drive::End(
                                shared.model_ctx.typed_outcome(MODEL_NOT_OFFERED, None),
                            );
                        }
                        ModelStep::Set(config_id) => {
                            let set = set_config_option(ID_MODEL, &sid, &config_id, &model);
                            if let Err(end) = queued(shared, "model", set) {
                                return Drive::End(end);
                            }
                            // The session does not exist for a prompt yet:
                            // one given meanwhile is kept as the first.
                            state.options = options;
                        }
                        ModelStep::SetModel => {
                            // The draft's way: the model alone, answered
                            // empty — the options kept are the session's own.
                            let set = set_model(ID_MODEL, &sid, &model);
                            if let Err(end) = queued(shared, "model", set) {
                                return Drive::End(end);
                            }
                            state.options = options;
                        }
                    }
                }
                ID_MODEL => {
                    // The whole option list again, as the model leaves it —
                    // the session's own when the answer carries none (the
                    // draft's `session/set_model` answers nothing at all).
                    let kept = std::mem::take(&mut state.options);
                    let options = result
                        .get("configOptions")
                        .filter(|o| o.is_array())
                        .cloned()
                        .unwrap_or(kept);
                    if let Err(end) = state.begin(shared, &options) {
                        return Drive::End(end);
                    }
                }
                other
                    if {
                        let mut ids = state.prompt_ids.locked();
                        let found = ids.contains(&other);
                        ids.retain(|p| *p != other);
                        found
                    } =>
                {
                    let stop = result
                        .get("stopReason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("end_turn");
                    let outcome = match stop {
                        "end_turn" | "max_turn_requests" | "idle" => Outcome::Completed,
                        "cancelled" => Outcome::Aborted,
                        "refusal" => Outcome::Failed {
                            error: "agent refused".into(),
                        },
                        other => {
                            shared
                                .model_ctx
                                .outcome(other)
                                .unwrap_or_else(|| Outcome::Failed {
                                    error: format!("stop reason: {other}"),
                                })
                        }
                    };
                    // Whatever the turn left open is over with it.
                    state.tools.clear();
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
                _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
            }
            return Drive::Continue;
        }
    }

    // Request or notification from the agent.
    match value.get("method").and_then(|m| m.as_str()) {
        // The conversation as it was, replayed while a session loads: kept
        // for the record, and said to nobody as if it were happening now.
        Some("session/update") if state.replaying => {
            shared.broadcaster.emit(SessionEvent::Raw(value));
        }
        Some("session/update") => {
            let update = value
                .pointer("/params/update")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match update.get("sessionUpdate").and_then(|v| v.as_str()) {
                Some("agent_message_chunk") => {
                    if let Some(text) = update.pointer("/content/text").and_then(|v| v.as_str()) {
                        shared
                            .broadcaster
                            .emit(SessionEvent::Progress(ProgressEvent::TextDelta {
                                text: text.to_string(),
                            }));
                    }
                }
                // The agent's reasoning as ACP streams it, beside its words.
                Some("agent_thought_chunk") => {
                    if let Some(text) = update.pointer("/content/text").and_then(|v| v.as_str()) {
                        shared.broadcaster.emit(SessionEvent::Progress(
                            ProgressEvent::ThinkingDelta {
                                text: text.to_string(),
                            },
                        ));
                    }
                }
                Some("tool_call") => {
                    let said = state.heard_of(&update);
                    // A call with no title is named by its kind, as before.
                    let name = match (&said.name, &said.kind) {
                        (None, Some(kind)) => kind.clone(),
                        _ => said.name(),
                    };
                    let tier = said.tier();
                    shared.set_activity(name.clone());
                    shared
                        .broadcaster
                        .emit(SessionEvent::Progress(ProgressEvent::ToolStarted {
                            name,
                            args_summary: String::new(),
                            tier,
                            id: tool_call_id(&update),
                        }));
                }
                Some("tool_call_update") => {
                    let said = state.heard_of(&update);
                    if let Some(status) = update.get("status").and_then(|v| v.as_str()) {
                        if status == "completed" || status == "failed" {
                            state.forget(&update);
                            shared.broadcaster.emit(SessionEvent::Progress(
                                ProgressEvent::ToolEnded {
                                    name: said.name(),
                                    ok: status == "completed",
                                    id: tool_call_id(&update),
                                },
                            ));
                        }
                    }
                }
                _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
            }
        }
        // The agent asks before a tool runs and waits. The adapter never
        // decides: it surfaces the request, keeps the offered options, and the
        // facade's `answer` sends the response the engine chose.
        Some("session/request_permission") => {
            let id = value.get("id").cloned().unwrap_or(serde_json::Value::Null);
            // The call as the agent has described it so far: the request
            // may name its id and nothing else, and the guard judges the
            // tool, the tier and the input the call was announced with.
            let said = state.heard_of(
                value
                    .pointer("/params/toolCall")
                    .unwrap_or(&serde_json::Value::Null),
            );
            let tier = said.tier();
            let tool_name = said.name();
            let options = value
                .pointer("/params/options")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            state
                .pending
                .locked()
                .insert(id.to_string(), (id.clone(), options));
            shared.set_activity(format!("asked to run {tool_name}"));
            shared.set_phase(Phase::AwaitingInput);
            shared
                .broadcaster
                .emit(SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                    request: InputRequest::permission(
                        id.to_string(),
                        tool_name,
                        tier,
                        said.kind.unwrap_or_else(|| "unknown".to_string()),
                        said.raw_input,
                    ),
                }));
        }
        _ => shared.broadcaster.emit(SessionEvent::Raw(value)),
    }
    Drive::Continue
}

/// The tier of a tool's kind, by what it can change: reading, searching,
/// fetching and the agent's own reasoning change nothing; an edit, a move
/// and a removal change files; everything else — a command, a switch of
/// mode, a kind the protocol grows, a call that said none — is the most it
/// could be (the kinds: agentclientprotocol.com/protocol/tool-calls).
fn acp_kind_tier(kind: Option<&str>) -> ToolTier {
    match kind {
        Some("read") | Some("search") | Some("fetch") | Some("think") => ToolTier::Read,
        Some("edit") | Some("move") | Some("delete") => ToolTier::Write,
        _ => ToolTier::Exec,
    }
}

/// Whether an agent's answer to `initialize` says it loads sessions.
fn loads_sessions(initialized: &serde_json::Value) -> bool {
    initialized
        .pointer("/agentCapabilities/loadSession")
        .and_then(|v| v.as_bool())
        == Some(true)
}

impl AcpState {
    /// The session is what it will be — made or loaded, on its model: set
    /// the effort among the levels `options` offers, then let it begin.
    /// A message the queue cannot take ends the session: nothing would run
    /// the first prompt, or hold the effort, that never went out.
    fn begin(&mut self, shared: &Shared, options: &serde_json::Value) -> Result<(), Outcome> {
        // The effort is queued while the session is still starting, so no
        // prompt — the launch's or a caller's — can be written before it.
        let set = self.effort.take().and_then(|asked| {
            let (config_id, value) = effort_option(options, asked)?;
            let sid = shared.native_id()?;
            Some(set_config_option(ID_EFFORT, &sid, &config_id, &value))
        });
        if let Some(set) = set {
            queued(shared, "effort", set)?;
        }
        // The session exists: the prompt kept for it goes out now, and the
        // phase moves under the same lock the facade reads it behind.
        let first = {
            let mut beginning = self.beginning.locked();
            beginning.exists = true;
            let first = beginning.first.take();
            shared.set_phase(Phase::Idle);
            first
        };
        let Some((request, prompt)) = first else {
            return Ok(());
        };
        let Some(sid) = shared.native_id() else {
            return Ok(());
        };
        self.prompt_ids.locked().push(request);
        shared.set_phase(Phase::Turn);
        shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        let first = jsonrpc_request(
            request,
            "session/prompt",
            serde_json::json!({
                "sessionId": sid,
                "prompt": [{ "type": "text", "text": prompt }]
            }),
        );
        queued(shared, "first prompt", first)
    }

    /// What is known of the tool call a message speaks of, once the message
    /// has been heard: what the call's earlier messages said, and this one.
    /// A message that names no id, and a call past the bound, is what it
    /// carries and nothing more.
    fn heard_of(&mut self, said: &serde_json::Value) -> KnownTool {
        let mut heard = KnownTool::default();
        let Some(id) = said.get("toolCallId").and_then(|v| v.as_str()) else {
            heard.hears(said);
            return heard;
        };
        if !self.tools.contains_key(id) && self.tools.len() >= MAX_KNOWN_TOOLS {
            heard.hears(said);
            return heard;
        }
        let known = self.tools.entry(id.to_string()).or_default();
        known.hears(said);
        KnownTool {
            name: known.name.clone(),
            kind: known.kind.clone(),
            raw_input: known.raw_input.clone(),
        }
    }

    /// The call a message speaks of has ended.
    fn forget(&mut self, said: &serde_json::Value) {
        if let Some(id) = said.get("toolCallId").and_then(|v| v.as_str()) {
            self.tools.remove(id);
        }
    }
}

pub struct AcpSession {
    adapter_id: String,
    shared: Shared,
    next_id: AtomicU64,
    prompt_ids: Arc<std::sync::Mutex<Vec<u64>>>,
    pending: PendingPermissions,
    beginning: SharedBeginning,
}

/// The option that answers a permission request: for an allow the
/// `allow_once` option, else the first `allow*`; for a deny `reject_once`,
/// else the first `reject*`; with neither, the request is cancelled. Once
/// before always whatever order the agent lists them — Gemini CLI lists
/// *Allow for this session* first — so the guard's one allow never becomes a
/// standing grant it is not asked about again.
fn permission_outcome(options: &[serde_json::Value], allow: bool) -> serde_json::Value {
    fn kind_of(option: &serde_json::Value) -> &str {
        option.get("kind").and_then(|k| k.as_str()).unwrap_or("")
    }
    let want = if allow { "allow" } else { "reject" };
    let once = format!("{want}_once");
    let picked = options
        .iter()
        .find(|o| kind_of(o) == once)
        .or_else(|| options.iter().find(|o| kind_of(o).starts_with(want)))
        .and_then(|o| o.get("optionId").cloned());
    match picked {
        Some(option_id) => {
            serde_json::json!({ "outcome": { "outcome": "selected", "optionId": option_id } })
        }
        None => serde_json::json!({ "outcome": { "outcome": "cancelled" } }),
    }
}

#[async_trait]
impl HarnessSession for AcpSession {
    fn snapshot(&self) -> SessionSnapshot {
        self.shared.snapshot()
    }

    fn phase(&self) -> Phase {
        self.shared.phase()
    }

    async fn prompt(&self, input: PromptInput) -> Result<(), HarnessError> {
        {
            // A session that does not exist yet keeps its first prompt; the
            // driver sends it once the agent has answered. One may wait.
            //
            // That holds for a session that will never exist, too — one that
            // ended in its handshake: a model the agent does not offer, an
            // agent that went away. Its first prompt is taken, as every
            // harness started with its prompt on the command line takes
            // one, and how it ended is the last word of its stream, which a
            // subscriber hears whenever it comes. Refusing the prompt here
            // would hand the caller *session terminated* in place of the
            // reason — and a model wall would read as a failed step instead
            // of walking the plan to its next model.
            let mut beginning = self.beginning.locked();
            if !beginning.exists {
                if beginning.first.is_some() {
                    return Err(HarnessError::Busy);
                }
                let request = self.next_id.fetch_add(1, Ordering::Relaxed);
                beginning.first = Some((request, input.text));
                return Ok(());
            }
        }
        if self.shared.phase() == Phase::Ended {
            return Err(HarnessError::Terminated);
        }
        if self.shared.phase() == Phase::Turn {
            return Err(HarnessError::Busy);
        }
        let sid = self.shared.native_id().ok_or(HarnessError::Busy)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.prompt_ids.locked().push(id);
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Progress(ProgressEvent::TurnStarted));
        self.shared
            .send(OutMsg::Json(jsonrpc_request(
                id,
                "session/prompt",
                serde_json::json!({ "sessionId": sid, "prompt": [{ "type": "text", "text": input.text }] }),
            )))
            .await
    }

    async fn steer(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("steer"))
    }

    async fn follow_up(&self, _msg: Steer) -> Result<(), HarnessError> {
        Err(HarnessError::NotSupported("follow_up"))
    }

    async fn answer(&self, request_id: &str, answer: InputAnswer) -> Result<(), HarnessError> {
        let (id, options) = self.pending.locked().remove(request_id).ok_or_else(|| {
            HarnessError::protocol(format!("no permission request {request_id} is waiting"))
        })?;
        let allow = !matches!(answer, InputAnswer::Deny { .. });
        self.shared
            .send(OutMsg::Json(jsonrpc_response(
                &id,
                permission_outcome(&options, allow),
            )))
            .await?;
        self.shared.set_phase(Phase::Turn);
        self.shared
            .broadcaster
            .emit(SessionEvent::Lifecycle(LifecycleEvent::InputResolved {
                id: request_id.to_string(),
            }));
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        if let Some(sid) = self.shared.native_id() {
            self.shared
                .send(OutMsg::Json(serde_json::json!({
                    "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": sid }
                })))
                .await
        } else {
            self.shared.send(OutMsg::Kill).await
        }
    }

    fn subscribe(&self) -> BoxEventStream {
        self.shared.broadcaster.subscribe()
    }

    fn resume_token(&self) -> Option<ResumeToken> {
        self.shared.resume_token(&self.adapter_id)
    }

    async fn dispose(self: Box<Self>) -> Result<(), HarnessError> {
        self.shared.close_stdin_quietly().await;
        Ok(())
    }
}

#[cfg(test)]
mod effort_tests {
    use super::*;
    use serde_json::json;

    /// A session's config options as an agent advertises them: a mode, and
    /// an effort under whatever id and spelling the agent likes.
    fn advertised(values: &[&str]) -> serde_json::Value {
        let options: Vec<serde_json::Value> = values
            .iter()
            .map(|v| json!({ "value": v, "name": v }))
            .collect();
        json!([
            {
                "id": "mode", "name": "Mode", "category": "mode", "type": "select",
                "currentValue": "ask",
                "options": [{ "value": "ask", "name": "Ask" }, { "value": "high", "name": "High" }]
            },
            {
                "id": "thinking", "name": "Thinking", "category": "thought_level",
                "type": "select", "currentValue": values.first(), "options": options
            }
        ])
    }

    fn picked(values: &[&str], asked: Effort) -> Option<(String, String)> {
        effort_option(&advertised(values), asked)
    }

    fn set(id: &str, value: &str) -> Option<(String, String)> {
        Some((id.to_string(), value.to_string()))
    }

    #[test]
    fn a_value_is_matched_to_a_level_whatever_its_case_and_separators() {
        for (word, level) in [
            ("high", Effort::High),
            ("High", Effort::High),
            ("HIGH", Effort::High),
            ("xhigh", Effort::Xhigh),
            ("XHigh", Effort::Xhigh),
            ("x-high", Effort::Xhigh),
            ("x_high", Effort::Xhigh),
            ("X High", Effort::Xhigh),
            ("Minimal", Effort::Minimal),
            ("MAX", Effort::Max),
            ("me-di-um", Effort::Medium),
        ] {
            assert_eq!(level_of(word), Some(level), "{word}");
        }
        // A word that is no level, an agent's own included.
        for word in ["", "off", "none", "auto", "ultra", "extra high", "high!"] {
            assert_eq!(level_of(word), None, "{word}");
        }
    }

    #[test]
    fn the_value_sent_is_the_agents_own_spelling_of_the_level() {
        let values = ["Low", "Medium", "High", "X-High"];
        assert_eq!(picked(&values, Effort::High), set("thinking", "High"));
        assert_eq!(picked(&values, Effort::Xhigh), set("thinking", "X-High"));
        assert_eq!(picked(&values, Effort::Low), set("thinking", "Low"));
        // The option is found by its category, never by its id: the mode
        // option offers a `high` of its own and is left alone.
        assert_eq!(picked(&["high"], Effort::High), set("thinking", "high"));
    }

    #[test]
    fn the_level_is_clamped_among_what_the_session_advertises() {
        let values = ["low", "medium", "high"];
        // The nearest below.
        assert_eq!(picked(&values, Effort::Max), set("thinking", "high"));
        assert_eq!(picked(&values, Effort::Xhigh), set("thinking", "high"));
        // Nothing below: the lowest above.
        assert_eq!(picked(&values, Effort::Minimal), set("thinking", "low"));
        assert_eq!(
            picked(&["max", "high"], Effort::Low),
            set("thinking", "high")
        );
        // A value that is no level is passed over, and the rest still count.
        let mixed = ["off", "low", "turbo", "high"];
        assert_eq!(picked(&mixed, Effort::Medium), set("thinking", "low"));
        assert_eq!(picked(&mixed, Effort::Max), set("thinking", "high"));
        // Values in groups count as values.
        let grouped = json!([{
            "id": "effort", "category": "thought_level", "type": "select",
            "options": [
                { "group": "fast", "name": "Fast", "options": [{ "value": "low", "name": "Low" }] },
                { "group": "deep", "name": "Deep", "options": [{ "value": "max", "name": "Max" }] }
            ]
        }]);
        assert_eq!(effort_option(&grouped, Effort::High), set("effort", "low"));
        assert_eq!(effort_option(&grouped, Effort::Max), set("effort", "max"));
    }

    #[test]
    fn nothing_is_set_when_no_thought_level_option_is_offered_or_none_is_a_level() {
        let only_mode = json!([{
            "id": "mode", "category": "mode", "type": "select",
            "options": [{ "value": "high", "name": "High" }]
        }]);
        for asked in Effort::ALL {
            assert_eq!(effort_option(&only_mode, asked), None);
            assert_eq!(effort_option(&json!([]), asked), None);
            assert_eq!(effort_option(&serde_json::Value::Null, asked), None);
            assert_eq!(picked(&[], asked), None);
            assert_eq!(picked(&["off", "on"], asked), None);
        }
        // An option with no id cannot be set.
        let nameless = json!([{
            "category": "thought_level",
            "options": [{ "value": "high" }]
        }]);
        assert_eq!(effort_option(&nameless, Effort::High), None);
    }

    /// A session mid-handshake over a channel the test reads, with no
    /// process behind it.
    fn handshake(
        effort: Option<Effort>,
        prompt: Option<&str>,
    ) -> (Shared, AcpState, mpsc::Receiver<OutMsg>) {
        handshake_on(None, effort, prompt)
    }

    /// The same, for a session asked to run `model`.
    fn handshake_on(
        model: Option<&str>,
        effort: Option<Effort>,
        prompt: Option<&str>,
    ) -> (Shared, AcpState, mpsc::Receiver<OutMsg>) {
        let (out_tx, out_rx) = mpsc::channel(8);
        let model = model.map(str::to_string);
        let shared = Shared::new(out_tx, "acp:test", model.clone(), "/work/here").at_effort(effort);
        let state = AcpState {
            beginning: Arc::new(std::sync::Mutex::new(Beginning {
                exists: false,
                first: prompt.map(|p| (ID_FIRST_PROMPT, p.to_string())),
            })),
            model,
            effort,
            options: serde_json::Value::Null,
            load_session: None,
            replaying: false,
            cwd: "/work/here".into(),
            mcp_servers: json!([]),
            prompt_ids: Arc::default(),
            pending: Arc::default(),
            tools: HashMap::new(),
        };
        (shared, state, out_rx)
    }

    /// What a run of the agent's messages came to, in order.
    fn heard(messages: Vec<serde_json::Value>) -> Vec<SessionEvent> {
        use futures::{FutureExt as _, StreamExt as _};
        let (shared, mut state, _out_rx) = handshake(None, None);
        let mut stream = shared.broadcaster.subscribe();
        for message in messages {
            map_acp(&shared, &mut state, message);
        }
        let mut events = Vec::new();
        while let Some(Some(event)) = stream.next().now_or_never() {
            events.push(event);
        }
        events
    }

    fn update(update: serde_json::Value) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0", "method": "session/update",
            "params": { "sessionId": "sess-1", "update": update }
        })
    }

    fn asks(id: u64, tool_call: serde_json::Value) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0", "id": id, "method": "session/request_permission",
            "params": {
                "sessionId": "sess-1", "toolCall": tool_call,
                "options": [
                    { "optionId": "ok", "name": "Allow", "kind": "allow_once" },
                    { "optionId": "no", "name": "Reject", "kind": "reject_once" }
                ]
            }
        })
    }

    /// What was heard of tools and of questions, as short words.
    fn told(events: &[SessionEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::Progress(ProgressEvent::ToolStarted { name, tier, .. }) => {
                    Some(format!("started {name} ({tier:?})"))
                }
                SessionEvent::Progress(ProgressEvent::ToolEnded { name, ok, .. }) => {
                    Some(format!("ended {name} ({ok})"))
                }
                SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                    match &request.kind {
                        bisa_harness::InputKind::Permission {
                            tool_name,
                            tier,
                            input,
                            ..
                        } => Some(format!("asked {tool_name} ({tier:?}) {input}")),
                        other => Some(format!("{other:?}")),
                    }
                }
                _ => None,
            })
            .collect()
    }

    /// The protocol lets every message about a tool call after the first
    /// name its id and nothing else ("All fields except `toolCallId` are
    /// optional in updates", and a permission request's `toolCall` is an
    /// update — agentclientprotocol.com/protocol/tool-calls and /schema,
    /// read 2026-09-30). What the first message said is what the call is:
    /// the guard judges the tool, the tier and the input it was given, and
    /// the tool that ends is the tool that started.
    #[test]
    fn a_tool_call_is_known_by_its_id_whatever_a_later_message_leaves_out() {
        let events = heard(vec![
            update(json!({
                "sessionUpdate": "tool_call", "toolCallId": "c1", "title": "Write notes.md",
                "kind": "edit", "status": "pending",
                "rawInput": { "path": "notes.md" }
            })),
            asks(7, json!({ "toolCallId": "c1" })),
            update(
                json!({ "sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "in_progress" }),
            ),
            update(
                json!({ "sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "completed" }),
            ),
        ]);
        assert_eq!(
            told(&events),
            [
                "started Write notes.md (Write)",
                r#"asked Write notes.md (Write) {"path":"notes.md"}"#,
                "ended Write notes.md (true)",
            ]
        );
    }

    #[test]
    fn a_tool_keeps_the_name_it_started_under_and_is_forgotten_once_it_ends() {
        // A question first, the call after it; a title said again, in other
        // words; an input said late. The name is the first one heard — it is
        // what the roster opened and what the review bracketed.
        let events = heard(vec![
            asks(
                7,
                json!({ "toolCallId": "c2", "title": "Run the tests", "kind": "execute" }),
            ),
            update(json!({
                "sessionUpdate": "tool_call", "toolCallId": "c2", "title": "Running the tests",
                "status": "in_progress", "rawInput": { "command": "fake-tool --all" }
            })),
            update(json!({
                "sessionUpdate": "tool_call_update", "toolCallId": "c2",
                "title": "Ran the tests", "status": "failed"
            })),
            // Ended and forgotten: the same id again is a call nobody knows.
            update(
                json!({ "sessionUpdate": "tool_call_update", "toolCallId": "c2", "status": "completed" }),
            ),
            // And one that was never announced says what it carries.
            update(json!({
                "sessionUpdate": "tool_call_update", "toolCallId": "c3",
                "title": "Read a file", "status": "completed"
            })),
        ]);
        assert_eq!(
            told(&events),
            [
                "asked Run the tests (Exec) null",
                "started Run the tests (Exec)",
                "ended Run the tests (false)",
                "ended ? (true)",
                "ended Read a file (true)",
            ]
        );
    }

    #[test]
    fn a_turns_end_forgets_the_calls_it_left_open_and_the_memory_has_a_bound() {
        let (shared, mut state, _out_rx) = handshake(None, None);
        for n in 0..(MAX_KNOWN_TOOLS + 50) {
            map_acp(
                &shared,
                &mut state,
                update(json!({
                    "sessionUpdate": "tool_call", "toolCallId": format!("c{n}"),
                    "title": "Read", "kind": "read"
                })),
            );
        }
        assert_eq!(
            state.tools.len(),
            MAX_KNOWN_TOOLS,
            "never more than the bound"
        );
        state.prompt_ids.locked().push(41);
        map_acp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": 41, "result": { "stopReason": "end_turn" } }),
        );
        assert!(
            state.tools.is_empty(),
            "a turn that ended left nothing open"
        );
    }

    #[test]
    fn a_kind_is_the_tier_of_what_it_can_change() {
        for (kind, tier) in [
            ("read", ToolTier::Read),
            ("search", ToolTier::Read),
            ("fetch", ToolTier::Read),
            ("think", ToolTier::Read),
            ("edit", ToolTier::Write),
            ("move", ToolTier::Write),
            ("delete", ToolTier::Write),
            ("execute", ToolTier::Exec),
            ("switch_mode", ToolTier::Exec),
            ("other", ToolTier::Exec),
            ("a kind nobody knows", ToolTier::Exec),
        ] {
            assert_eq!(acp_kind_tier(Some(kind)), tier, "{kind}");
        }
        assert_eq!(
            acp_kind_tier(None),
            ToolTier::Exec,
            "unsaid is the most it could be"
        );
    }

    fn written(out_rx: &mut mpsc::Receiver<OutMsg>) -> Vec<serde_json::Value> {
        let mut lines = Vec::new();
        while let Ok(msg) = out_rx.try_recv() {
            match msg {
                OutMsg::Json(v) => lines.push(v),
                other => panic!("{other:?}"),
            }
        }
        lines
    }

    fn session_new(config_options: serde_json::Value) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0", "id": ID_SESSION,
            "result": { "sessionId": "sess-1", "configOptions": config_options }
        })
    }

    #[test]
    fn the_effort_is_set_before_the_first_prompt() {
        let (shared, mut state, mut out_rx) = handshake(Some(Effort::Max), Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new(advertised(&["Low", "Medium", "High"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            lines[0],
            json!({
                "jsonrpc": "2.0", "id": ID_EFFORT, "method": "session/set_config_option",
                "params": { "sessionId": "sess-1", "configId": "thinking", "value": "High" }
            })
        );
        assert_eq!(lines[1]["id"], ID_FIRST_PROMPT);
        assert_eq!(lines[1]["method"], "session/prompt");
        assert_eq!(ID_EFFORT, 4);
        // The answer to it is nobody's turn: the session goes on.
        let answered =
            json!({ "jsonrpc": "2.0", "id": ID_EFFORT, "result": { "configOptions": [] } });
        assert!(matches!(
            map_acp(&shared, &mut state, answered),
            Drive::Continue
        ));
        assert_eq!(shared.phase(), Phase::Turn);
        assert!(written(&mut out_rx).is_empty());
    }

    #[test]
    fn a_session_opened_idle_is_set_before_it_takes_a_prompt() {
        let (shared, mut state, mut out_rx) = handshake(Some(Effort::Low), None);
        assert_eq!(shared.phase(), Phase::Starting);
        map_acp(
            &shared,
            &mut state,
            session_new(advertised(&["low", "high"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/set_config_option");
        assert_eq!(lines[0]["params"]["value"], "low");
        assert_eq!(shared.phase(), Phase::Idle);
    }

    #[test]
    fn nothing_is_sent_when_no_effort_is_set_or_the_agent_offers_none() {
        // None asked.
        let (shared, mut state, mut out_rx) = handshake(None, Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new(advertised(&["low", "high"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/prompt");

        // Asked, and the agent has no `thought_level` option.
        let (shared, mut state, mut out_rx) = handshake(Some(Effort::High), Some("go"));
        let only_mode = json!([{
            "id": "mode", "category": "mode", "type": "select",
            "options": [{ "value": "high", "name": "High" }]
        }]);
        map_acp(&shared, &mut state, session_new(only_mode));
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/prompt");

        // Asked, and the agent advertises no config options at all.
        let (shared, mut state, mut out_rx) = handshake(Some(Effort::High), None);
        map_acp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": ID_SESSION, "result": { "sessionId": "sess-1" } }),
        );
        assert!(written(&mut out_rx).is_empty());
        assert_eq!(shared.phase(), Phase::Idle);
    }

    /// A session's config options as an agent that chooses its model
    /// advertises them: the model it is on among the ones it offers, and the
    /// levels that model takes.
    fn on_model(current: &str, models: &[&str], levels: &[&str]) -> serde_json::Value {
        let choices = |values: &[&str]| -> Vec<serde_json::Value> {
            values
                .iter()
                .map(|v| json!({ "value": v, "name": v }))
                .collect()
        };
        json!([
            {
                "id": "model", "name": "Model", "category": "model", "type": "select",
                "currentValue": current, "options": choices(models)
            },
            {
                "id": "effort", "name": "Effort", "category": "thought_level",
                "type": "select", "currentValue": levels.first(), "options": choices(levels)
            }
        ])
    }

    fn model_answered(config_options: serde_json::Value) -> serde_json::Value {
        json!({ "jsonrpc": "2.0", "id": ID_MODEL, "result": { "configOptions": config_options } })
    }

    fn refused(id: u64, message: &str) -> serde_json::Value {
        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": message } })
    }

    /// The result of a `session/new` with these config options.
    fn answered(config_options: serde_json::Value) -> serde_json::Value {
        json!({ "sessionId": "sess-1", "configOptions": config_options })
    }

    /// The result of a `session/new` as an agent on the draft before config
    /// options gives it (Gemini CLI): the models it lists, each an id and a
    /// label, the one the session is on — and no config options at all.
    fn on_the_draft(current: &str, models: &[&str]) -> serde_json::Value {
        let listed: Vec<serde_json::Value> = models
            .iter()
            .map(|m| json!({ "modelId": m, "name": format!("Model {m}") }))
            .collect();
        json!({
            "sessionId": "sess-1",
            "modes": {
                "availableModes": [{ "id": "default", "name": "Default" }],
                "currentModeId": "default"
            },
            "models": { "availableModels": listed, "currentModelId": current }
        })
    }

    fn session_new_on_the_draft(current: &str, models: &[&str]) -> serde_json::Value {
        json!({ "jsonrpc": "2.0", "id": ID_SESSION, "result": on_the_draft(current, models) })
    }

    #[test]
    fn what_a_sessions_options_say_of_a_model() {
        let options = answered(on_model("small", &["small", "large"], &["low"]));
        assert_eq!(
            model_step(&options, "large"),
            ModelStep::Set("model".into())
        );
        assert_eq!(
            model_step(&options, "small"),
            ModelStep::Nothing,
            "already on it"
        );
        assert_eq!(model_step(&options, "huge"), ModelStep::NotOffered);
        // A model is a name: nothing near it is taken for it.
        assert_eq!(model_step(&options, "Large"), ModelStep::NotOffered);
        assert_eq!(model_step(&options, "larg"), ModelStep::NotOffered);
        // A session with no model option cannot be told, and is not refused.
        for silent in [
            answered(advertised(&["low", "high"])),
            answered(json!([])),
            json!({ "sessionId": "sess-1" }),
            serde_json::Value::Null,
        ] {
            assert_eq!(model_step(&silent, "large"), ModelStep::Nothing);
        }
        // The option is found by its category, never by its id; values in
        // groups count as values.
        let grouped = answered(json!([{
            "id": "engine", "category": "model", "type": "select", "currentValue": "a",
            "options": [
                { "group": "fast", "name": "Fast", "options": [{ "value": "a" }] },
                { "group": "deep", "name": "Deep", "options": [{ "value": "b" }] }
            ]
        }]));
        assert_eq!(model_step(&grouped, "b"), ModelStep::Set("engine".into()));
        // An option with no id cannot be set, and refuses nothing.
        let nameless = answered(json!([{ "category": "model", "options": [{ "value": "a" }] }]));
        assert_eq!(model_step(&nameless, "b"), ModelStep::Nothing);
        assert_eq!(ID_MODEL, 5);
    }

    #[test]
    fn what_a_sessions_models_say_of_a_model_when_it_offers_no_model_option() {
        let draft = on_the_draft("auto", &["auto", "gemini-2.5-pro"]);
        assert_eq!(
            model_step(&draft, "auto"),
            ModelStep::Nothing,
            "on it already"
        );
        assert_eq!(model_step(&draft, "gemini-2.5-pro"), ModelStep::SetModel);
        assert_eq!(model_step(&draft, "gemini-9"), ModelStep::NotOffered);
        // A listed model's name is a label, never its id.
        assert_eq!(
            model_step(&draft, "Model gemini-2.5-pro"),
            ModelStep::NotOffered
        );
        // An agent that offers both is read by its `model` config option.
        let mut both = draft.clone();
        both["configOptions"] = on_model("small", &["small", "large"], &["low"]);
        assert_eq!(model_step(&both, "large"), ModelStep::Set("model".into()));
        assert_eq!(
            model_step(&both, "gemini-2.5-pro"),
            ModelStep::NotOffered,
            "the option's list, not the draft's"
        );
        // Models said without a list cannot be told.
        let mut no_list = draft.clone();
        no_list["models"] = json!({ "currentModelId": "auto" });
        assert_eq!(model_step(&no_list, "gemini-2.5-pro"), ModelStep::Nothing);
        // No current model said: a listed one is set, the default among them too.
        let mut no_current = draft.clone();
        no_current["models"]
            .as_object_mut()
            .unwrap()
            .remove("currentModelId");
        assert_eq!(
            model_step(&no_current, "gemini-2.5-pro"),
            ModelStep::SetModel
        );
        assert_eq!(model_step(&no_current, "auto"), ModelStep::SetModel);
    }

    #[test]
    fn a_model_is_set_with_set_model_where_the_session_offers_no_option_and_the_prompt_waits() {
        // The draft's answer to a set is empty — `null` or `{}` alike.
        for empty in [serde_json::Value::Null, json!({})] {
            let (shared, mut state, mut out_rx) =
                handshake_on(Some("gemini-2.5-pro"), Some(Effort::High), Some("go"));
            map_acp(
                &shared,
                &mut state,
                session_new_on_the_draft("auto", &["auto", "gemini-2.5-pro"]),
            );
            let lines = written(&mut out_rx);
            assert_eq!(
                lines,
                [json!({
                    "jsonrpc": "2.0", "id": ID_MODEL, "method": "session/set_model",
                    "params": { "sessionId": "sess-1", "modelId": "gemini-2.5-pro" }
                })],
                "the model alone, the draft's way: no prompt before its answer"
            );
            assert!(!state.beginning.locked().exists);
            assert_eq!(shared.phase(), Phase::Starting);
            // Answered: the session offers no level, so the effort asked is
            // sent nowhere, and the prompt goes out.
            let answered = json!({ "jsonrpc": "2.0", "id": ID_MODEL, "result": empty });
            assert!(matches!(
                map_acp(&shared, &mut state, answered),
                Drive::Continue
            ));
            let lines = written(&mut out_rx);
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert_eq!(lines[0]["method"], "session/prompt");
            assert!(state.beginning.locked().exists);
            assert_eq!(shared.phase(), Phase::Turn);
        }
    }

    #[test]
    fn a_model_the_draft_list_does_not_offer_or_refuses_ends_as_unavailable_before_any_prompt() {
        // Not among the ones it lists.
        let (shared, mut state, mut out_rx) = handshake_on(Some("gemini-9"), None, Some("go"));
        let end = map_acp(
            &shared,
            &mut state,
            session_new_on_the_draft("auto", &["auto", "gemini-2.5-pro"]),
        );
        match end {
            Drive::End(Outcome::ModelUnavailable { model, reason, .. }) => {
                assert_eq!(model, "gemini-9");
                assert_eq!(reason, MODEL_NOT_OFFERED);
            }
            _ => panic!("the session went on with a model nobody asked for"),
        }
        assert!(written(&mut out_rx).is_empty(), "nothing was sent");

        // Listed, and refused when set.
        let (shared, mut state, mut out_rx) =
            handshake_on(Some("gemini-2.5-pro"), None, Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new_on_the_draft("auto", &["auto", "gemini-2.5-pro"]),
        );
        assert_eq!(written(&mut out_rx).len(), 1);
        let end = map_acp(&shared, &mut state, refused(ID_MODEL, "Model not found"));
        assert!(matches!(
            end,
            Drive::End(Outcome::ModelUnavailable { reason, .. })
                if reason == "the agent refused the model: Model not found"
        ));
        assert!(written(&mut out_rx).is_empty(), "no prompt was written");

        // An agent that listed its models yet has no `session/set_model` is
        // a failure to say, not a model to walk past.
        let (shared, mut state, _out_rx) = handshake_on(Some("gemini-2.5-pro"), None, Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new_on_the_draft("auto", &["auto", "gemini-2.5-pro"]),
        );
        let no_method = json!({
            "jsonrpc": "2.0", "id": ID_MODEL,
            "error": { "code": -32601, "message": "Method not found" }
        });
        assert!(matches!(
            map_acp(&shared, &mut state, no_method),
            Drive::End(Outcome::Failed { error })
                if error == "the agent cannot set a model: Method not found"
        ));
    }

    #[test]
    fn a_loaded_session_is_set_to_its_model_through_the_draft_too() {
        let (shared, mut state, mut out_rx) = handshake_on(Some("gemini-2.5-pro"), None, None);
        state.load_session = Some("sess-7".into());
        state.replaying = true;
        // The answer to `session/load` carries no id: the one asked for stands.
        let mut loaded = on_the_draft("auto", &["auto", "gemini-2.5-pro"]);
        loaded.as_object_mut().unwrap().remove("sessionId");
        map_acp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": ID_SESSION, "result": loaded }),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/set_model");
        assert_eq!(
            lines[0]["params"],
            json!({ "sessionId": "sess-7", "modelId": "gemini-2.5-pro" })
        );
        assert!(!state.replaying);
    }

    #[test]
    fn an_allow_is_once_before_always_whatever_order_the_agent_lists() {
        // Gemini CLI's order: *Allow for this session* before *Allow*.
        let gemini = [
            json!({ "optionId": "proceed_always", "name": "Allow for this session", "kind": "allow_always" }),
            json!({ "optionId": "proceed_once", "name": "Allow", "kind": "allow_once" }),
            json!({ "optionId": "cancel", "name": "Reject", "kind": "reject_once" }),
        ];
        assert_eq!(
            permission_outcome(&gemini, true)["outcome"]["optionId"],
            "proceed_once"
        );
        assert_eq!(
            permission_outcome(&gemini, false)["outcome"]["optionId"],
            "cancel"
        );
        // An agent that offers only a standing answer: still the answer, the
        // only one there is.
        let always_only = [
            json!({ "optionId": "always", "kind": "allow_always" }),
            json!({ "optionId": "never", "kind": "reject_always" }),
        ];
        assert_eq!(
            permission_outcome(&always_only, true)["outcome"]["optionId"],
            "always"
        );
        assert_eq!(
            permission_outcome(&always_only, false)["outcome"]["optionId"],
            "never"
        );
        // Nothing to pick: the request is cancelled.
        assert_eq!(
            permission_outcome(&[], true),
            json!({ "outcome": { "outcome": "cancelled" } })
        );
    }

    #[test]
    fn the_model_is_set_first_and_the_effort_is_fitted_from_the_models_answer() {
        let (shared, mut state, mut out_rx) =
            handshake_on(Some("large"), Some(Effort::Max), Some("go"));
        // The session opens on `small`, which takes two levels.
        map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low", "medium"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(
            lines,
            [json!({
                "jsonrpc": "2.0", "id": ID_MODEL, "method": "session/set_config_option",
                "params": { "sessionId": "sess-1", "configId": "model", "value": "large" }
            })],
            "the model alone: no effort and no prompt before its answer"
        );
        assert!(
            !state.beginning.locked().exists,
            "no prompt may be written yet"
        );
        assert_eq!(shared.phase(), Phase::Starting);

        // `large` takes more: the level asked is held to *its* levels.
        map_acp(
            &shared,
            &mut state,
            model_answered(on_model(
                "large",
                &["small", "large"],
                &["low", "high", "max"],
            )),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0]["id"], ID_EFFORT);
        assert_eq!(
            lines[0]["params"],
            json!({ "sessionId": "sess-1", "configId": "effort", "value": "max" }),
            "fitted to the first list, it would have been `medium`"
        );
        assert_eq!(lines[1]["id"], ID_FIRST_PROMPT);
        assert_eq!(lines[1]["method"], "session/prompt");
        assert!(state.beginning.locked().exists);
        assert_eq!(shared.phase(), Phase::Turn);
    }

    #[test]
    fn a_models_answer_that_carries_no_list_leaves_the_sessions_own_to_fit_the_effort() {
        let (shared, mut state, mut out_rx) = handshake_on(Some("large"), Some(Effort::High), None);
        map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low", "medium"])),
        );
        assert_eq!(written(&mut out_rx).len(), 1);
        map_acp(
            &shared,
            &mut state,
            json!({ "jsonrpc": "2.0", "id": ID_MODEL, "result": {} }),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["params"]["value"], "medium");
        assert_eq!(shared.phase(), Phase::Idle);
    }

    #[test]
    fn nothing_is_sent_for_a_model_the_session_is_on_or_cannot_be_told() {
        // On it already: the effort and the prompt, at once.
        let (shared, mut state, mut out_rx) =
            handshake_on(Some("small"), Some(Effort::Low), Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low", "medium"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0]["id"], ID_EFFORT);
        assert_eq!(lines[1]["method"], "session/prompt");

        // No model option: the agent runs its own, as it always did.
        let (shared, mut state, mut out_rx) = handshake_on(Some("large"), None, Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new(advertised(&["low", "high"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/prompt");

        // No model asked: the option is left where the agent has it.
        let (shared, mut state, mut out_rx) = handshake_on(None, None, Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low"])),
        );
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/prompt");
    }

    #[test]
    fn a_model_the_session_does_not_offer_or_refuses_ends_as_unavailable_before_any_prompt() {
        // Not among the ones it chooses from.
        let (shared, mut state, mut out_rx) =
            handshake_on(Some("huge"), Some(Effort::High), Some("go"));
        let end = map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low"])),
        );
        match end {
            Drive::End(Outcome::ModelUnavailable {
                model,
                reason,
                retry_after,
            }) => {
                assert_eq!(model, "huge");
                assert_eq!(reason, MODEL_NOT_OFFERED);
                assert_eq!(retry_after, None);
            }
            _ => panic!("the session went on with a model nobody asked for"),
        }
        assert!(written(&mut out_rx).is_empty(), "nothing was sent");
        assert!(!state.beginning.locked().exists);

        // Offered, and refused when set.
        let (shared, mut state, mut out_rx) = handshake_on(Some("large"), None, Some("go"));
        map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low"])),
        );
        assert_eq!(written(&mut out_rx).len(), 1);
        let end = map_acp(
            &shared,
            &mut state,
            refused(ID_MODEL, "not enabled for this account"),
        );
        match end {
            Drive::End(Outcome::ModelUnavailable { model, reason, .. }) => {
                assert_eq!(model, "large");
                assert_eq!(
                    reason,
                    "the agent refused the model: not enabled for this account"
                );
            }
            _ => panic!("a refused model is the model's failure"),
        }
        assert!(written(&mut out_rx).is_empty(), "no prompt was written");

        // A refusal in the provider's own words keeps them.
        let (shared, mut state, _out_rx) = handshake_on(Some("large"), None, None);
        map_acp(
            &shared,
            &mut state,
            session_new(on_model("small", &["small", "large"], &["low"])),
        );
        let end = map_acp(&shared, &mut state, refused(ID_MODEL, "insufficient_quota"));
        assert!(matches!(
            end,
            Drive::End(Outcome::ModelUnavailable { reason, .. }) if reason.starts_with("quota exhausted")
        ));

        // An effort that is refused is still only said: the session goes on.
        let (shared, mut state, _out_rx) = handshake(Some(Effort::High), None);
        map_acp(
            &shared,
            &mut state,
            session_new(advertised(&["low", "high"])),
        );
        assert!(matches!(
            map_acp(&shared, &mut state, refused(ID_EFFORT, "no")),
            Drive::Continue
        ));
    }

    /// The outbound queue is bounded, and a protocol message it cannot take
    /// is a step of the handshake lost: nothing would ever answer it. The
    /// session ends saying which — never left *starting*, or holding a
    /// first prompt nobody will run, until a clock somewhere else notices.
    #[test]
    fn a_protocol_message_the_queue_cannot_take_ends_the_session_in_words() {
        let initialized = json!({
            "jsonrpc": "2.0", "id": ID_INIT,
            "result": { "protocolVersion": 1, "agentCapabilities": {} }
        });
        let opened = json!({
            "jsonrpc": "2.0", "id": ID_SESSION,
            "result": { "sessionId": "sess-1" }
        });
        // A queue of one, already full: the agent stopped reading its input.
        let full = || {
            let (out_tx, out_rx) = mpsc::channel(1);
            out_tx
                .try_send(OutMsg::Json(json!({ "jsonrpc": "2.0", "method": "noop" })))
                .unwrap();
            (out_tx, out_rx)
        };
        // `session/new` cannot go out.
        let (out_tx, _out_rx) = full();
        let shared = Shared::new(out_tx, "acp:test", None, "/work/here");
        let (_, mut state, _) = handshake(None, Some("go"));
        match map_acp(&shared, &mut state, initialized.clone()) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("session/new"), "{error}");
                assert!(error.contains("not queued"), "{error}");
            }
            other => panic!("the session went on with its handshake lost: {other:?}"),
        }
        // The session exists, and the first prompt kept for it cannot go out.
        let (out_tx, _out_rx) = full();
        let shared = Shared::new(out_tx, "acp:test", None, "/work/here");
        let (_, mut state, _) = handshake(None, Some("go"));
        match map_acp(&shared, &mut state, opened) {
            Drive::End(Outcome::Failed { error }) => {
                assert!(error.contains("first prompt"), "{error}");
            }
            other => panic!("a first prompt nobody will run was kept: {other:?}"),
        }
    }

    #[test]
    fn a_revival_is_asked_only_of_an_agent_that_loads_sessions() {
        let initialized = |capabilities: serde_json::Value| {
            json!({
                "jsonrpc": "2.0", "id": ID_INIT,
                "result": { "protocolVersion": 1, "agentCapabilities": capabilities }
            })
        };
        // It says it can: the load is asked, with where and what to mount.
        let (shared, mut state, mut out_rx) = handshake(None, None);
        state.load_session = Some("sess-7".into());
        assert!(matches!(
            map_acp(
                &shared,
                &mut state,
                initialized(json!({ "loadSession": true }))
            ),
            Drive::Continue
        ));
        let lines = written(&mut out_rx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["method"], "session/load");
        assert_eq!(lines[0]["params"]["sessionId"], "sess-7");

        // It does not — unsaid, or said false: nothing is asked, and the
        // session ends in a sentence of its own.
        for capabilities in [json!({}), json!({ "loadSession": false })] {
            let (shared, mut state, mut out_rx) = handshake(None, None);
            state.load_session = Some("sess-7".into());
            match map_acp(&shared, &mut state, initialized(capabilities)) {
                Drive::End(Outcome::Failed { error }) => assert_eq!(error, CANNOT_LOAD),
                _ => panic!("a load was asked of an agent that loads none"),
            }
            assert!(written(&mut out_rx).is_empty());
        }

        // A new session asks nothing of the capability.
        let (shared, mut state, mut out_rx) = handshake(None, None);
        map_acp(&shared, &mut state, initialized(json!({})));
        assert_eq!(written(&mut out_rx)[0]["method"], "session/new");
    }

    #[test]
    fn an_agent_gone_before_its_session_exists_has_failed_to_start() {
        let beginning: SharedBeginning = Arc::default();
        match clean_exit(&beginning, &[]) {
            Outcome::Failed { error } => {
                assert_eq!(error, "the agent ended before its session began");
            }
            other => panic!("{other:?}"),
        }
        match clean_exit(&beginning, &["Install it? [y/N]".into(), "bye".into()]) {
            Outcome::Failed { error } => assert_eq!(
                error,
                "the agent ended before its session began; stderr tail: Install it? [y/N] | bye"
            ),
            other => panic!("{other:?}"),
        }
        // Once the session exists, leaving quietly is leaving done.
        beginning.locked().exists = true;
        assert!(matches!(clean_exit(&beginning, &[]), Outcome::Completed));
    }

    #[test]
    fn a_command_is_started_in_the_sessions_directory_with_the_sessions_environment() {
        let command = AcpCommand::new("acp:test", "agent-bin", ["acp", "--stdio"]);
        let mut spec = revival_spec(&ResumeToken {
            adapter_id: "acp:test".into(),
            native_id: "sess-1".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: None,
            effort: None,
        });
        spec.env.insert("KEPT".into(), "1".into());
        spec.env_remove.push("TAKEN_AWAY".into());
        let proc = proc_spec(&command, &spec);
        assert_eq!(proc.program, "agent-bin");
        assert_eq!(proc.args, ["acp", "--stdio"]);
        assert_eq!(proc.env.get("KEPT").map(String::as_str), Some("1"));
        assert_eq!(proc.env_remove, ["TAKEN_AWAY"]);
        // The generic target's command is its four words, and nothing of a
        // model or an effort: both reach an agent the protocol's way.
        let target = AcpAdapter::new("acp:goose", "Goose (ACP)", "goose", vec!["acp".into()]);
        assert_eq!(
            target.command(),
            AcpCommand::new("acp:goose", "goose", ["acp"])
        );
    }

    #[test]
    fn a_token_revives_only_the_adapter_that_minted_it() {
        let token = ResumeToken {
            adapter_id: "acp:test".into(),
            native_id: "sess-1".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: Some("large".into()),
            effort: Some(Effort::High),
        };
        let spec = revival("acp:test", &token).unwrap();
        assert_eq!(spec.model.as_deref(), Some("large"));
        assert_eq!(spec.effort, Some(Effort::High));
        assert!(matches!(
            revival("copilot", &token),
            Err(HarnessError::Protocol(_))
        ));
    }

    #[test]
    fn every_level_may_be_asked_for_before_a_session_says_what_it_takes() {
        let adapter = AcpAdapter::generic();
        assert!(adapter.caps().contains(HarnessCaps::EFFORT));
        assert_eq!(adapter.efforts(None), Effort::ALL.to_vec());
        assert_eq!(adapter.efforts(Some("anything")), Effort::ALL.to_vec());
    }

    #[test]
    fn a_revived_session_is_given_the_effort_it_ran_with() {
        let token = ResumeToken {
            adapter_id: "acp:test".into(),
            native_id: "sess-1".into(),
            cwd: std::path::PathBuf::from("/work/here"),
            transcript_path: None,
            model: Some("model-1".into()),
            effort: Some(Effort::Xhigh),
        };
        let spec = revival_spec(&token);
        assert_eq!(spec.cwd, token.cwd);
        assert_eq!(spec.model, token.model);
        assert_eq!(spec.effort, Some(Effort::Xhigh));
        assert!(spec.prompt.is_empty());

        // And a session's own token says what it was launched with.
        let (shared, _state, _out_rx) = handshake(Some(Effort::Xhigh), None);
        shared.set_native_id("sess-1");
        let minted = shared.resume_token("acp:test").unwrap();
        assert_eq!(minted.effort, Some(Effort::Xhigh));
        assert_eq!(minted.native_id, "sess-1");
    }
}
