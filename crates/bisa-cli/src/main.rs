//! The `bisa` multicall binary.
//!
//! Personalities: the CLI itself, `bisa node` (the daemon), and
//! `bisa mcp` (stdio MCP server injected into harness sessions).
//! Discipline: JSON on stdout with `--json`, human-readable otherwise;
//! activity lines go to stderr so stdout stays machine-parseable.
//!
//! A goal has a mode: `bisa new "…"` captures in `goals.default_mode`
//! (`auto` unless the workspace says otherwise) — the Workflow Agent designs
//! a workflow and the platform adopts, starts and repairs it alone; `--mode
//! guided` has you adopt the proposal from the inbox; `--mode manual` leaves
//! the design to you. `--workflow` names the workflow yourself and starts it
//! at once unless `--no-start`; the run then walks its steps and asks you
//! only where a step is yours. A goal whose workflow begins on events
//! *listens* instead: each occurrence starts a run on it, and `bisa status`
//! says what it hears.

mod activity;
mod addons;
mod agent_context;
mod agents;
mod catalog;
mod client;
mod connector;
mod conversations;
mod ctx;
mod decisions;
mod files;
mod gitsetup;
mod inputs;
mod listening;
mod localize;
mod mcp;
mod net;
mod output;
mod projects;
mod reindex;
mod run;
mod security;
mod session;
mod settings;
mod signals;
mod skills;
mod step;
mod studio;
mod tags;
mod target;
mod usage;
#[cfg(test)]
mod verbs;
mod workflow;

use anyhow::{bail, Context as _, Result};
use bisa_core::workitem::WorkItemSpec;
use bisa_core::{Answer, ClosureReason, Gate, GoalId, RunId, WorkflowRun};
use bisa_engine::{DecideOutcome, SubmitRequest};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use ctx::Ctx;
use output::Out;
use serde_json::json;
use std::str::FromStr;

#[derive(Parser)]
#[command(
    name = "bisa",
    version,
    about = "From goal to working outcome — orchestrating your coding harnesses",
    after_help = concat!(
        "Website: ",
        env!("CARGO_PKG_HOMEPAGE"),
        " · Source: ",
        env!("CARGO_PKG_REPOSITORY")
    )
)]
struct Cli {
    /// Workspace data directory (default: ~/.bisa)
    #[arg(long, global = true)]
    data_dir: Option<std::path::PathBuf>,
    /// Machine-readable JSON output on stdout
    #[arg(long, global = true)]
    json: bool,
    /// Force file-based key storage (no OS keyring). Hidden: used by tests
    /// and headless servers.
    #[arg(long, global = true, hide = true)]
    file_keys: bool,
    /// Never route through a running `bisa node` daemon
    #[arg(long, global = true)]
    no_node: bool,
    /// The language to speak (a tag such as `en`); the environment's
    /// (`LC_ALL`, `LC_MESSAGES`, `LANG`) when absent, English when neither
    /// names a language the platform ships
    #[arg(long, global = true)]
    lang: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize the workspace and owner identity
    Init,
    /// Capture a new goal — the Workflow Agent designs its workflow, or you
    /// name one
    New {
        /// Statement of the goal
        statement: String,
        #[arg(long)]
        title: Option<String>,
        /// How the goal moves: `auto` (the Workflow Agent designs and the
        /// platform adopts, starts and repairs alone), `guided` (it proposes,
        /// you adopt) or `manual` (you design). Default: `goals.default_mode`
        #[arg(long, value_parser = parse_mode)]
        mode: Option<bisa_core::GoalMode>,
        /// Run this workflow: an installed workflow's id or a catalog slug
        /// (installed on the way). Without it the Workflow Agent designs one
        /// (or, in manual mode, you do).
        #[arg(long)]
        workflow: Option<String>,
        /// Run input, `name=value` (repeatable; a JSON value or a bare string)
        #[arg(long = "input")]
        inputs: Vec<String>,
        /// With `--workflow`: do not start the run yet — `bisa run` starts it
        #[arg(long)]
        no_start: bool,
        /// Who carries the goal: `agent:<id>` / `team:<id>` / `human:<64-hex>`
        /// (repeatable)
        #[arg(long = "assignee")]
        assignees: Vec<String>,
        /// A file given as the goal's context — a brief, a spec, a screenshot
        /// — kept under the goal's `documents/` folder (repeatable)
        #[arg(long = "document", value_name = "PATH")]
        documents: Vec<std::path::PathBuf>,
        #[command(flatten)]
        tags: tags::TagSetArgs,
    },
    /// Answer a question the General Agent (or a working agent) asked you
    Answer {
        /// A goal's id, a run's, or the specific gate id
        id: String,
        /// Your answer in free text. Optional when you pick an option or say
        /// you are not sure.
        text: Option<String>,
        /// An option id the question offered (repeatable). Ids come from the
        /// question itself; an id it never offered is refused, not dropped.
        #[arg(long = "option", short = 'o')]
        options: Vec<String>,
        /// You do not know. Not a decline: the agent is steered to ask
        /// something narrower rather than told to stop.
        #[arg(long)]
        unsure: bool,
        /// Which waiting step is meant, when the run has several `human`
        /// or `approval` steps waiting and no gate id was given
        #[arg(long)]
        step: Option<String>,
    },
    /// List pending approvals
    Inbox,
    /// Decide a pending gate (pass a goal's id, or a run's)
    Approve {
        id: String,
        /// Reject instead of approving
        #[arg(long)]
        no: bool,
        #[arg(long)]
        rationale: Option<String>,
        /// Run input for an adoption, `name=value` (repeatable)
        #[arg(long = "input")]
        inputs: Vec<String>,
        /// Which waiting step is meant, when the run has several `approval`
        /// or `human` steps waiting
        #[arg(long)]
        step: Option<String>,
    },
    /// Begin a goal's work and follow it — or follow the live run: a goal
    /// whose workflow begins on events listens, any other runs; `--new`
    /// makes another run, queued behind the live one
    Run {
        id: String,
        /// Run input, `name=value` (repeatable) — or, for a goal that
        /// listens, what its event runs bind
        #[arg(long = "input")]
        inputs: Vec<String>,
        /// The start step a run begins at: the start by hand is a run now,
        /// an event start a test run
        #[arg(long)]
        start: Option<String>,
        /// A test run's sample event as JSON: what the start's mapping reads,
        /// as if it had happened
        #[arg(long, requires = "start")]
        data: Option<String>,
        /// Render a live activity timeline
        #[arg(long)]
        watch: bool,
        /// A new run even while one is live: queued behind it, started on
        /// its own when that one ends
        #[arg(long)]
        new: bool,
    },
    /// Stop a goal: it stops listening, its sessions ended, its queued runs
    /// withdrawn, its live run cancelled; the goal stays open, ready for a
    /// new run
    Stop {
        id: String,
        /// Why, in a sentence — kept on the run
        #[arg(long)]
        rationale: Option<String>,
    },
    /// Restart a goal: a new run of its last run's workflow and inputs,
    /// started at once; a live run is cancelled first
    Restart {
        id: String,
        /// Render a live activity timeline
        #[arg(long)]
        watch: bool,
    },
    /// Every run of a goal, newest first — the queued ones with their place
    Runs { id: String },
    /// Ask the Workflow Agent to design a goal's workflow again — after a
    /// stall, a failure or a restart
    Design { goal: String },
    /// Replace the not-yet-started steps of a goal's running workflow
    Amend {
        goal: String,
        /// The whole workflow as it should read now (JSON or TOML, `-` stdin)
        #[arg(long)]
        from: String,
    },
    /// Workflows: the library, and what a goal runs
    Workflow {
        #[command(subcommand)]
        command: workflow::WorkflowCmd,
    },
    /// Connectors: outside platforms a `connector` step calls, and this
    /// machine's accounts for them
    Connector {
        #[command(subcommand)]
        command: connector::ConnectorCmd,
    },
    /// One step of a run — a goal's, or a run of the workspace: answer,
    /// release, done
    Step {
        #[command(subcommand)]
        command: step::StepCmd,
    },
    /// Show a goal's status, workflow and run — or one run's, by its id
    Status { id: String },
    /// Render a goal's journal — or a run of the workspace's — as an
    /// activity timeline
    Log {
        id: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// The diagnostic log on this machine: the folder, each process's files,
    /// the crash reports and the newest of them — what to attach to a bug
    /// report
    Logs,
    /// Where this workspace is: the data directory and the log folder,
    /// without opening anything
    Paths,
    /// List or search goals. With no query, every goal; `--tag` narrows
    /// either. This is the goal list.
    Search {
        query: Option<String>,
        #[command(flatten)]
        filter: tags::TagFilterArgs,
    },
    /// Harness catalog operations
    Harness {
        #[command(subcommand)]
        command: HarnessCmd,
    },
    /// The reporter personality: a harness's own hooks run `session report`
    /// inside a terminal the desktop opened, so the roster knows what the
    /// harness is doing. Not for people to type.
    #[command(hide = true)]
    Session {
        #[command(subcommand)]
        command: session::SessionCmd,
    },
    /// MCP servers: the registry agents draw on.
    ///
    /// With no subcommand and `--socket`, this is the other personality of the
    /// word: the stdio MCP server the engine injects into harness sessions.
    /// The engine spawns that form by name, so the registry verbs live under
    /// it rather than replacing it.
    Mcp {
        #[command(subcommand)]
        command: Option<mcp::McpCmd>,
        /// Run the stdio MCP server against the engine's socket at this path,
        /// rather than a registry command. The engine passes it.
        #[arg(long)]
        socket: Option<std::path::PathBuf>,
        /// Work-item scope (worker sessions)
        #[arg(long, conflicts_with_all = ["goal", "conversation"])]
        work_item: Option<String>,
        /// Goal scope (guided-mode sessions); with --conversation, the goal
        /// that thread is about
        #[arg(long)]
        goal: Option<String>,
        /// Conversation scope (an agent's chat instance): the scope ULID
        #[arg(long, requires = "agent")]
        conversation: Option<String>,
        /// The agent this session runs as: required by --conversation, and
        /// optional on --goal, where a human may drive the cycle by hand
        #[arg(long)]
        agent: Option<String>,
    },
    /// The catalog: agents, skills, teams and channels you can install
    Catalog {
        #[command(subcommand)]
        command: catalog::CatalogCmd,
    },
    /// Agent definitions: prompt + harness + model + skills + MCP servers
    Agent {
        #[command(subcommand)]
        command: agents::AgentCmd,
    },
    /// Skills: the shared library of procedures agents follow
    Skill {
        #[command(subcommand)]
        command: skills::SkillCmd,
    },
    /// Addons: the overlay widgets installed here, and the switches on each
    Addon {
        #[command(subcommand)]
        command: addons::AddonCmd,
    },
    /// Tags: the one filing system, and what carries which tag
    Tags(tags::TagsArgs),
    /// Teams: agents and humans working a goal together
    Team {
        #[command(subcommand)]
        command: agents::TeamCmd,
    },
    /// Put agents, humans or teams on a goal (repeatable assignee args)
    Assign {
        /// Goal id
        goal: String,
        /// `agent:<id>` / `human:<64-hex>` / `team:<id>` (one or more)
        #[arg(required = true)]
        assignees: Vec<String>,
        /// Replace the list instead of adding to it
        #[arg(long)]
        replace: bool,
    },
    /// Take assignees off a goal (no names clears the whole list)
    Unassign {
        goal: String,
        assignees: Vec<String>,
    },
    /// Projects: the folders a goal owns, with or without git
    Project {
        #[command(subcommand)]
        command: projects::ProjectCmd,
    },
    /// Workstreams: a branch and a checkout where one piece of work happens
    Workstream {
        #[command(subcommand)]
        command: projects::WorkstreamCmd,
    },
    /// Read the folders a goal, project or workstream owns
    Files {
        #[command(subcommand)]
        command: files::FilesCmd,
    },
    /// Your git setup: profiles by organization, SSH for git hosts, the code
    /// host accounts, and what a checkout will use to reach its remote
    Git {
        #[command(subcommand)]
        command: gitsetup::GitCmd,
    },
    /// Live harness sessions (the runtime roster)
    Sessions {
        #[command(subcommand)]
        command: agents::SessionCmd,
    },
    /// The Decision-Making Agent as this node runs it: who answers, the
    /// judgements it made, a question to try it with, a remote provider's key
    Decisions {
        #[command(subcommand)]
        command: decisions::DecisionsCmd,
    },
    /// What the platform needs before it can work — git, a harness, and the
    /// Decision-Making, General and Workflow Agents — each with the official
    /// way to fix it. Exit 1 while something is missing.
    Doctor,
    /// The Redactor, the Guard and the Classifier as this node runs them, and
    /// two previews to try a rule on
    Security {
        #[command(subcommand)]
        command: security::SecurityCmd,
    },
    /// An agent's memory (owner view)
    Recall {
        #[command(subcommand)]
        command: agents::RecallCmd,
    },
    /// Standing conversations
    Channels {
        #[command(subcommand)]
        command: studio::ChannelCmd,
    },
    /// Conversations: a saved exchange with agents, with an origin —
    /// listed, started, read, spoken into, titled, put away, deleted
    Conversation {
        #[command(subcommand)]
        command: conversations::ConversationCmd,
    },
    /// Post a message to a scope (a channel, a goal or a conversation id)
    Msg {
        scope: String,
        text: String,
        #[arg(long)]
        reply_to: Option<String>,
        /// Address a pubkey, an agent id, or this channel's own id — which
        /// expands to its roster (repeatable). An agent answers when it is
        /// addressed and not before.
        #[arg(long = "mention")]
        mentions: Vec<String>,
        /// A file to hand over as a file (repeatable).
        #[arg(long = "attach")]
        attachments: Vec<std::path::PathBuf>,
        /// A file to share as an artifact — rendered live where it is read —
        /// as `<path>` or `<path>:<title>` (repeatable, at most 8).
        #[arg(long = "artifact")]
        artifacts: Vec<String>,
    },
    /// Read a scope's messages (marks it read)
    Msgs {
        scope: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        before: Option<u64>,
    },
    /// Direct conversations: list them, or send one
    Dm {
        #[command(subcommand)]
        command: studio::DmCmd,
    },
    /// The workspace activity feed
    Pulse {
        #[arg(long, default_value_t = 40)]
        limit: usize,
        /// One concept — workspace, goals, workflows, projects, channels, agents, node — or all
        #[arg(long, default_value = "all")]
        concept: String,
    },
    /// Mark a conversation read
    Read { scope: String },
    /// Mark a conversation unread
    Unread { scope: String },
    /// The relays this node talks through (`sync.relays`) and direct peers
    Relay {
        #[command(subcommand)]
        command: RelayCmd,
    },
    /// People, invitations and the workspaces you are a guest of
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCmd,
    },
    /// Run the headless daemon
    Node {
        /// Additional loopback TCP bind, e.g. 127.0.0.1:7317
        #[arg(long)]
        listen: Option<std::net::SocketAddr>,
        /// Permit --listen on a non-loopback address. Every route asks for the
        /// workspace's token, and the token crosses that network unencrypted:
        /// whoever reads it holds the whole workspace. Only pass it behind your
        /// own tunnel or firewall.
        #[arg(long)]
        insecure_allow_remote: bool,
    },
    /// Put a goal, a workflow or a project away — or take it back out with
    /// --undo. An open goal is closed first; a project's sessions are stopped.
    Archive {
        /// goal · workflow · project
        kind: String,
        id: String,
        /// Take it back out instead
        #[arg(long)]
        undo: bool,
    },
    /// Delete a goal — every session on it stopped, its unfinished run
    /// cancelled, its journal, runs and work items gone. What became of the
    /// projects born of it is --projects; --tree moves a deleted managed
    /// folder to the Trash. A project merely attached is detached, never deleted.
    Rm {
        id: String,
        /// keep (the default) · archive · delete
        #[arg(long, default_value = "keep")]
        projects: String,
        /// With --projects delete: move each managed folder to the Trash
        #[arg(long)]
        tree: bool,
    },
    /// Close a goal: cancels its unfinished run and its work items and
    /// releases its workstreams (checkouts stay on disk)
    Close {
        id: String,
        /// Why it is being given up on
        #[arg(long)]
        rationale: Option<String>,
        /// The goal that replaces this one
        #[arg(long)]
        superseded_by: Option<String>,
    },
    /// Signals: the durable record of what a workflow's events heard, and the
    /// named ones raised by hand
    Signal {
        #[command(subcommand)]
        command: signals::SignalCmd,
    },
    /// Settings: three scopes, one registry
    Settings {
        #[command(subcommand)]
        command: settings::SettingsCmd,
    },
    /// What an agent session can rely on: every tool per session kind, and the chat framing
    AgentContext,
    /// Gate governance: who may sign which gate
    Governance {
        #[command(subcommand)]
        command: GovernanceCmd,
    },
}

#[derive(Subcommand)]
enum GovernanceCmd {
    /// Show the current gate policies
    Show,
    /// Set a gate's policy.
    ///
    /// `gate` is approval | escalation | publish; `policy` is
    /// owner | admins | members | <pubkey|team:id>[,...].
    ///
    /// `publish` is the gate on pushing a branch or opening a pull request.
    /// Unlike the others it never widens to a goal's assignees on the
    /// default owner-only policy, because approving it spends the owner's git
    /// and code host credentials — setting it here is the only way to let somebody
    /// else publish.
    Set { gate: String, policy: String },
}

#[derive(Subcommand)]
enum RelayCmd {
    /// Add a relay URL to `sync.relays`; a running node connects at once
    Add { url: String },
    /// The configured relays — with their health when a node runs
    List,
    /// Remove a relay URL from `sync.relays`
    Remove { url: String },
    /// Try a relay once — the one named, or every configured relay — whether or not the wire is on
    Check { url: Option<String> },
    /// Why a relay does not connect: the switch, TLS, the proxy bound, then a check of every configured relay
    Doctor,
    /// Add a direct peer by hand (member pubkey + endpoint id + socket addr) under `sync.iroh.peers`
    PeerAdd {
        member_pubkey: String,
        node_id: String,
        addr: String,
    },
}

#[derive(Subcommand)]
enum WorkspaceCmd {
    /// Make a single-use invitation: prints the link and the code
    Invite {
        /// admin | member | guest
        #[arg(long, default_value = "guest")]
        role: String,
        /// A standing channel a guest is put on (repeatable)
        #[arg(long = "channel")]
        channels: Vec<String>,
        /// What to call the person until they say
        #[arg(long)]
        label: Option<String>,
    },
    /// Every invitation and where it stands
    Invites,
    /// Withdraw a pending invitation
    Revoke { id: String },
    /// Join a workspace from an invitation link or code
    Join {
        code: String,
        /// What to be called there
        #[arg(long)]
        label: Option<String>,
    },
    /// The workspaces you are a guest of
    Hosts,
    /// Leave a workspace you are a guest of
    Leave { host: String },
    /// Everyone in this workspace: the owner and the people hosted here
    People,
    /// Change a person's role: admin | member | guest
    Role { pubkey: String, role: String },
    /// Remove a person. What they already received cannot be un-sent.
    Remove { pubkey: String },
    /// Throw the index away and rebuild it from the truth files — for a
    /// cache you have reason to doubt. Refused while a node holds the workspace.
    Reindex,
}

#[derive(Subcommand)]
enum HarnessCmd {
    /// List all known harnesses with probe results
    List,
    /// Probe one harness by id
    Probe { id: String },
}

#[tokio::main]
async fn main() {
    // Before anything can open a TLS connection: the process's one crypto
    // provider. rustls cannot choose between the two backends this workspace
    // compiles in, and a client built without one panics (`bisa_collab::tls`).
    bisa_collab::ensure_crypto_provider();
    // The language first — `--lang` from the raw arguments, else the
    // environment — so the help clap prints is already in it (`localize`).
    let raw: Vec<String> = std::env::args().collect();
    let locale = localize::locale_from_args(&raw);
    let cli =
        Cli::from_arg_matches(&localize::localize(Cli::command(), &locale, "bisa").get_matches())
            .unwrap_or_else(|e| e.exit());
    let out = Out::new(cli.json, locale.clone());
    // One subscriber for the process: stderr under `RUST_LOG` as before, and
    // the file under the workspace's `logs/` — errors only until a workspace
    // is open and the machine's `logging.*` settings are read.
    let process = process_of(&cli.command);
    let log = bisa_log::install(process, env!("CARGO_PKG_VERSION"));
    let ctx = Ctx::new(
        cli.data_dir,
        cli.file_keys,
        cli.no_node,
        log.clone(),
        locale,
    );
    // Where the file goes. The node and a one-shot command write under
    // their own workspace; a child personality — an MCP server, a hook —
    // is spawned without `--data-dir` and writes only where its parent
    // said (`BISA_LOG_DIR`), so a test's child never reaches
    // `~/.bisa` and a person's never guesses.
    let dir = match process {
        bisa_log::Process::Node | bisa_log::Process::Cli => {
            Some(bisa_store::Paths::new(&ctx.data_dir).logs_dir())
        }
        bisa_log::Process::Mcp => bisa_log::env_dir(),
        bisa_log::Process::Desktop => None,
    };
    if let Some(dir) = dir {
        if let Err(e) = log.attach(dir, bisa_log::LogConfig::default()) {
            tracing::warn!("no log file: {e}");
        }
    }
    // The goodbye takes this run's marker away, so the next start of the
    // family has nothing to report about it.
    match dispatch(cli.command, &ctx, &out).await {
        Ok(()) => log.goodbye("finished"),
        Err(e) => {
            tracing::error!(target: "bisa_cli", "{e:#}");
            out.error(&localize::said(&out, &e));
            log.goodbye("failed");
            std::process::exit(1);
        }
    }
}

/// `bisa paths`: where this workspace is. Nothing is opened or made —
/// the desktop shell asks this before it starts the node, to attach its own
/// log under the workspace's folder without spelling a path itself.
fn paths(ctx: &Ctx, out: &Out) -> Result<()> {
    let paths = bisa_store::Paths::new(&ctx.data_dir);
    let data_dir = paths.root().display().to_string();
    let logs_dir = paths.logs_dir().display().to_string();
    out.json_value(json!({ "data_dir": data_dir, "logs_dir": logs_dir }));
    out.say(&bisa_core::text!(
        "cli-main-data-dir-logs-dir",
        data_dir = data_dir.to_string(),
        logs_dir = logs_dir.to_string()
    ));
    Ok(())
}

/// `bisa logs`: the diagnostic log on this machine — the folder, each
/// family's files newest first, the crash reports and the newest of them.
/// The same answer as the node's `GET /logs`, without a node.
fn logs(ctx: &Ctx, out: &Out) -> Result<()> {
    let root = bisa_store::Paths::new(&ctx.data_dir).logs_dir();
    let listing = bisa_log::list(&root).with_context(|| {
        bisa_core::text!(
            "cli-main-reading-log-folder",
            a0 = (root.display()).to_string()
        )
    })?;
    let latest = bisa_log::latest_crash(&root);
    out.json_value(json!({
        "dir": root.display().to_string(),
        "families": listing.families.iter().map(|f| json!({
            "process": f.process.prefix(),
            "dir": f.dir.display().to_string(),
            "files": f.files,
        })).collect::<Vec<_>>(),
        "crashes": listing.crashes,
        "bytes": listing.bytes(),
        "latest_crash": latest.as_ref().map(|(name, r)| json!({
            "name": name,
            "process": r.process.prefix(),
            "kind": r.kind.as_str(),
            "at": r.at,
            "message": r.message,
        })),
    }));
    let mut text = bisa_i18n::say(&bisa_core::text!(
        "cli-main-log-folder",
        a0 = (root.display()).to_string()
    ));
    for family in &listing.families {
        if family.files.is_empty() {
            continue;
        }
        text.push_str(&format!("\n\n{}/", family.process.prefix()));
        for file in &family.files {
            text.push_str(&format!("\n  {}  {} bytes", file.name, file.bytes));
        }
    }
    if !listing.crashes.is_empty() {
        text.push_str("\n\ncrashes/");
        for file in &listing.crashes {
            text.push_str(&format!("\n  {}  {} bytes", file.name, file.bytes));
        }
    }
    match latest {
        Some((name, r)) => text.push_str(&bisa_i18n::say(&bisa_core::text!(
            "cli-main-newest-crash",
            name = name.to_string(),
            a0 = (r.process.prefix()).to_string(),
            a1 = (r.kind.as_str()).to_string(),
            a2 = (r.at).to_string(),
            a3 = (r.message).to_string()
        ))),
        None => text.push_str(&bisa_i18n::say(&bisa_core::text!(
            "cli-main-no-crash-report"
        ))),
    }
    out.human(&text);
    Ok(())
}

/// Which file family this invocation writes: the daemon's, a child
/// personality's — the MCP server a session is handed, the reporter and
/// guard hooks a terminal harness runs — or a one-shot command's.
fn process_of(command: &Command) -> bisa_log::Process {
    match command {
        Command::Node { .. } => bisa_log::Process::Node,
        Command::Mcp { command: None, .. } | Command::Session { .. } => bisa_log::Process::Mcp,
        _ => bisa_log::Process::Cli,
    }
}

async fn dispatch(cmd: Command, ctx: &Ctx, out: &Out) -> Result<()> {
    match cmd {
        Command::Init => init(ctx, out),
        Command::New {
            statement,
            title,
            mode,
            workflow,
            inputs,
            no_start,
            assignees,
            documents,
            tags,
        } => {
            new_goal(
                ctx,
                out,
                NewGoalInput {
                    statement,
                    title,
                    mode,
                    workflow,
                    inputs: inputs::Typed::parse(&inputs)?,
                    no_start,
                    assignees: projects::parse_assignees(&assignees)?,
                    tags: tags.parse()?,
                    documents,
                },
            )
            .await
        }
        Command::Answer {
            id,
            text,
            options,
            unsure,
            step,
        } => {
            answer(
                ctx,
                out,
                &id,
                text.as_deref(),
                &options,
                unsure,
                parse_step(step.as_deref())?,
            )
            .await
        }
        Command::Workflow { command } => workflow::workflow(ctx, out, command).await,
        Command::Connector { command } => connector::connector(ctx, out, command).await,
        Command::Step { command } => step::step(ctx, out, command).await,
        Command::Design { goal } => design(ctx, out, &goal).await,
        Command::Amend { goal, from } => amend(ctx, out, &goal, &from).await,
        Command::Inbox => inbox(ctx, out).await,
        Command::Approve {
            id,
            no,
            rationale,
            inputs,
            step,
        } => {
            approve(
                ctx,
                out,
                &id,
                !no,
                rationale.as_deref(),
                inputs::Typed::parse(&inputs)?,
                parse_step(step.as_deref())?,
            )
            .await
        }
        Command::Run {
            id,
            inputs,
            start,
            data,
            watch,
            new,
        } => {
            let begin = run::Begin::parse(start.as_deref(), data.as_deref())?;
            let inputs = inputs::Typed::parse(&inputs)?;
            run::run(ctx, out, &id, inputs, begin, watch, new).await
        }
        Command::Stop { id, rationale } => run::stop(ctx, out, &id, rationale).await,
        Command::Restart { id, watch } => run::restart(ctx, out, &id, watch).await,
        Command::Runs { id } => run::runs(ctx, out, &id).await,
        Command::Status { id } => status(ctx, out, &id).await,
        Command::Log { id, limit } => log(ctx, out, &id, limit).await,
        Command::Logs => logs(ctx, out),
        Command::Paths => paths(ctx, out),
        Command::Search { query, filter } => search(ctx, out, query.as_deref(), &filter),
        Command::Harness { command } => harness(ctx, out, command).await,
        Command::Session { command } => {
            session::session(command);
            Ok(())
        }
        Command::Mcp {
            command: Some(command),
            ..
        } => mcp::mcp(ctx, out, command).await,
        Command::Mcp {
            command: None,
            socket,
            work_item,
            goal,
            conversation,
            agent,
        } => {
            let socket = socket.context(
                "`bisa mcp` needs a subcommand (list | show | add | edit | rm | \
                 enable | disable), or --socket to run the stdio MCP server",
            )?;
            let scope = match (work_item, goal, conversation) {
                (Some(w), None, None) => bisa_mcp::Scope::WorkItem(w),
                // `--agent` is optional here: a human can drive a cycle by
                // hand, but when an Agent definition drives it the scope
                // carries the identity so the engine resolves the signer.
                (None, Some(i), None) => bisa_mcp::Scope::Goal { goal: i, agent },
                (None, goal, Some(c)) => {
                    let agent = agent
                        .context(bisa_core::text!("cli-main-conversation-requires-agent-id"))?;
                    bisa_mcp::Scope::Conversation {
                        scope: c,
                        agent,
                        goal,
                    }
                }
                _ => bail!(bisa_core::text!("cli-main-pass-exactly-one-work-item-ulid")),
            };
            bisa_mcp::run_stdio(socket, scope).await
        }
        Command::Catalog { command } => catalog::catalog(ctx, out, command).await,
        Command::Agent { command } => agents::agent(ctx, out, command).await,
        Command::Skill { command } => skills::skill(ctx, out, command).await,
        Command::Addon { command } => addons::addon(ctx, out, command).await,
        Command::Tags(args) => tags::tags(ctx, out, args),
        Command::Team { command } => agents::team(ctx, out, command).await,
        Command::Assign {
            goal,
            assignees,
            replace,
        } => projects::assign(ctx, out, &goal, &assignees, replace).await,
        Command::Unassign { goal, assignees } => {
            projects::unassign(ctx, out, &goal, &assignees).await
        }
        Command::Project { command } => projects::project(ctx, out, command).await,
        Command::Workstream { command } => projects::workstream(ctx, out, command).await,
        Command::Files { command } => files::files(ctx, out, command).await,
        Command::Git { command } => gitsetup::git(ctx, out, command).await,
        Command::Sessions { command } => agents::sessions(ctx, out, command).await,
        Command::Decisions { command } => decisions::decisions(ctx, out, command).await,
        Command::Doctor => doctor(ctx, out).await,
        Command::Security { command } => security::security(ctx, out, command).await,
        Command::Recall { command } => agents::recall(ctx, out, command),
        Command::Channels { command } => studio::channels(ctx, out, command).await,
        Command::Conversation { command } => conversations::conversation(ctx, out, command).await,
        Command::Msg {
            scope,
            text,
            reply_to,
            mentions,
            attachments,
            artifacts,
        } => {
            studio::post_message(
                ctx,
                out,
                &scope,
                &text,
                reply_to,
                &mentions,
                &attachments,
                &artifacts,
            )
            .await
        }
        Command::Msgs {
            scope,
            limit,
            before,
        } => studio::messages(ctx, out, &scope, limit, before).await,
        Command::Dm { command } => studio::dm(ctx, out, command).await,
        Command::Pulse { limit, concept } => studio::pulse(ctx, out, limit, &concept).await,
        Command::Read { scope } => studio::read_marker(ctx, out, &scope, true).await,
        Command::Unread { scope } => studio::read_marker(ctx, out, &scope, false).await,
        Command::Relay { command } => match command {
            RelayCmd::Add { url } => net::relay_add(ctx, out, &url).await,
            RelayCmd::List => net::relay_list(ctx, out).await,
            RelayCmd::Remove { url } => net::relay_remove(ctx, out, &url).await,
            RelayCmd::Check { url } => net::relay_check(ctx, out, url.as_deref()).await,
            RelayCmd::Doctor => net::relay_doctor(ctx, out).await,
            RelayCmd::PeerAdd {
                member_pubkey,
                node_id,
                addr,
            } => net::peer_add(ctx, out, &member_pubkey, &node_id, &addr).await,
        },
        Command::Workspace { command } => match command {
            WorkspaceCmd::Invite {
                role,
                channels,
                label,
            } => net::invite(ctx, out, &role, &channels, label).await,
            WorkspaceCmd::Invites => net::invites(ctx, out).await,
            WorkspaceCmd::Revoke { id } => net::revoke(ctx, out, &id).await,
            WorkspaceCmd::Join { code, label } => net::join(ctx, out, &code, label).await,
            WorkspaceCmd::Hosts => net::hosts(ctx, out).await,
            WorkspaceCmd::Leave { host } => net::leave(ctx, out, &host).await,
            WorkspaceCmd::People => net::people(ctx, out).await,
            WorkspaceCmd::Role { pubkey, role } => net::set_role(ctx, out, &pubkey, &role).await,
            WorkspaceCmd::Remove { pubkey } => net::remove_person(ctx, out, &pubkey).await,
            WorkspaceCmd::Reindex => reindex::reindex(ctx, out),
        },
        Command::Node {
            listen,
            insecure_allow_remote,
        } => node(ctx, out, listen, insecure_allow_remote).await,
        Command::Close {
            id,
            rationale,
            superseded_by,
        } => close(ctx, out, &id, rationale, superseded_by).await,
        Command::Archive { kind, id, undo } => archive(ctx, out, &kind, &id, !undo).await,
        Command::Rm { id, projects, tree } => rm_goal(ctx, out, &id, &projects, tree).await,
        Command::Signal { command } => signals::signal(ctx, out, command).await,
        Command::Governance { command } => governance(ctx, out, command).await,
        Command::Settings { command } => settings::settings(ctx, out, command).await,
        Command::AgentContext => agent_context::run(out),
    }
}

fn init(ctx: &Ctx, out: &Out) -> Result<()> {
    let ws = ctx.workspace()?;
    let pubkey = ws.owner_principal().to_string();
    let npub = ctx.npub(&ws)?;
    out.say(&bisa_core::text!(
        "cli-main-workspace-owner-pubkey-owner-npub",
        a0 = (ws.root().display()).to_string(),
        pubkey = pubkey.to_string(),
        npub = npub.to_string()
    ));
    out.json_value(json!({"data_dir": ws.root(), "pubkey": pubkey, "npub": npub}));
    Ok(())
}

pub(crate) fn parse_goal_id(s: &str) -> Result<GoalId> {
    GoalId::from_str(s).with_context(|| {
        bisa_core::text!("cli-main-not-goal-id-expected-ulid", s = format!("{s:?}"))
    })
}

/// What the human does next, once a goal exists.
fn next_step_line(goal: &bisa_core::Goal, started: bool) -> String {
    let id = goal.id;
    // A goal whose workflow begins on events was armed, not run: each
    // occurrence starts a run on it from now on.
    if goal.listening.is_some() {
        return bisa_i18n::say(&bisa_core::text!(
            "cli-main-goal-listens-see-status",
            id = id.to_string()
        ));
    }
    match (goal.workflow, started) {
        (Some(wf), true) => bisa_i18n::say(&bisa_core::text!(
            "cli-main-running-workflow-follow-with-bisa-run",
            wf = wf.to_string(),
            id = id.to_string()
        )),
        (Some(wf), false) => bisa_i18n::say(&bisa_core::text!(
            "cli-main-workflow-chosen-start-with-bisa-run",
            wf = wf.to_string(),
            id = id.to_string()
        )),
        (None, _) => match goal.mode {
            bisa_core::GoalMode::Auto => bisa_i18n::say(&bisa_core::text!(
                "cli-main-workflow-agent-designing-workflow-will-start",
                id = id.to_string()
            )),
            bisa_core::GoalMode::Guided => bisa_i18n::say(&bisa_core::text!(
                "cli-main-workflow-agent-proposing-workflow-review-with",
                id = id.to_string()
            )),
            bisa_core::GoalMode::Manual => bisa_i18n::say(&bisa_core::text!(
                "cli-main-no-workflow-yet-design-goal-s",
                id = id.to_string()
            )),
        },
    }
}

/// `--mode`, as one of the three words; anything else is refused by name.
fn parse_mode(raw: &str) -> std::result::Result<bisa_core::GoalMode, String> {
    raw.parse::<bisa_core::GoalMode>()
        .map_err(|e| e.to_string())
}

/// What `bisa new` was told, in one value.
struct NewGoalInput {
    statement: String,
    title: Option<String>,
    mode: Option<bisa_core::GoalMode>,
    workflow: Option<String>,
    inputs: inputs::Typed,
    no_start: bool,
    assignees: Vec<bisa_core::Assignee>,
    tags: bisa_core::Tags,
    /// Files to give the goal as context, read from disk here.
    documents: Vec<std::path::PathBuf>,
}

/// Read each file into the workspace's content-addressed store and answer
/// its descriptor — the two steps the desktop takes over HTTP, taken here
/// against the shared workspace, so the node and the embedded engine alike
/// find the bytes held when they materialise the goal's `documents/`.
fn store_documents(
    ws: &bisa_store::Workspace,
    paths: &[std::path::PathBuf],
) -> Result<Vec<bisa_core::AttachmentRef>> {
    paths
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path).map_err(|e| {
                anyhow::anyhow!(bisa_core::text!(
                    "cli-main-cannot-read",
                    a0 = (path.display()).to_string(),
                    e = e.to_string()
                ))
            })?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            Ok(ws.put_attachment(&bytes, &name, bisa_core::mime_of_name(&name))?)
        })
        .collect()
}

/// Capture a goal. The daemon does this in-process when one is running so
/// the Workflow Agent wakes immediately (or the run starts where the work
/// will run); otherwise a short-lived embedded engine does it.
///
/// With `--workflow` the run starts at once unless `--no-start`; without one
/// the goal's mode says who designs: the Workflow Agent (auto, guided) or
/// the person (manual). `--mode` absent takes `goals.default_mode`.
async fn new_goal(ctx: &Ctx, out: &Out, input: NewGoalInput) -> Result<()> {
    let NewGoalInput {
        statement,
        title,
        mode,
        workflow,
        inputs,
        no_start,
        assignees,
        tags,
        documents,
    } = input;
    let start = workflow.is_some() && !no_start;
    let ws = ctx.workspace()?;
    // Its person's words, read by what the workflow asks for.
    let inputs = match workflow.as_deref() {
        Some(raw) => inputs.read_for(&ws, raw),
        None => inputs.read(&[]),
    };
    // The bytes first, whichever engine captures: a descriptor names
    // bytes the store already holds, or it names nothing.
    let documents = store_documents(&ws, &documents)?;
    drop(ws);

    if let Some(client) = ctx.node_client().await {
        let body = json!({
            "statement": statement,
            "title": title,
            "workflow": workflow,
            // The route starts the run when inputs travel with a workflow;
            // an empty map is still "start it".
            "inputs": if start { Some(&inputs) } else { None },
            "mode": mode,
            "assignees": assignees.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            "tags": tags,
            "documents": documents,
        });
        let v = client.post("/goals", body).await?;
        let goal: bisa_core::Goal = serde_json::from_value(v["goal"].clone()).context(
            bisa_core::text!("cli-main-unexpected-goal-payload-from-node"),
        )?;
        out.say(&bisa_core::text!(
            "cli-main-goal-captured",
            a0 = (goal.id).to_string(),
            a1 = (next_step_line(&goal, start)).to_string()
        ));
        let secrets = listening::rows(&v["secrets"]);
        say_minted(out, goal.id, secrets);
        out.json_value(json!({
            "goal": goal.id.to_string(), "mode": goal.mode,
            "workflow": goal.workflow, "run": goal.run,
            "secrets": secrets,
        }));
        return Ok(());
    }

    let ws = ctx.workspace()?;
    let workflow = match workflow.as_deref() {
        Some(raw) => Some(workflow::resolve_workflow(&ws, raw, true)?.id),
        None => None,
    };
    drop(ws);
    let (engine, _) = ctx.engine().await?;
    let mode = mode.unwrap_or_else(|| engine.default_goal_mode());
    let result = engine.submit_goal_showing(SubmitRequest {
        statement,
        title,
        budget: Default::default(),
        mode,
        origin: bisa_core::GoalOrigin::Captured,
        workflow,
        inputs,
        start,
        assignees,
        tags,
        documents,
    });
    let made = match result {
        Ok(made) => made,
        Err(e) => {
            engine.shutdown().await;
            return Err(e.into());
        }
    };
    let goal = made.goal;
    // A goal captured with a workflow of events listens at once: the secrets
    // its public hooks minted are shown below, once.
    let secrets: Vec<serde_json::Value> = made
        .secrets
        .iter()
        .filter_map(|secret| serde_json::to_value(secret).ok())
        .collect();
    let id = goal.id;
    // On a goal the Workflow Agent designs for, and without a daemon, this
    // process is the only engine there is — so hold it until its first wake
    // settles (a question or a proposal lands, or it gives up). Anything
    // less leaves the goal a draft with an empty inbox, which is exactly the
    // dead end designing exists to remove.
    if workflow.is_none() && mode.designs() {
        wait_for_design(&engine, id, out).await;
    }
    let goal = engine.workspace().get_goal(id).unwrap_or(goal);
    // A run started here — by `--workflow`, or by an auto goal adopting its
    // design — would die with this process: follow it to a settlement the
    // way `bisa run` does.
    if goal.run.is_some() {
        out.say(&bisa_core::text!(
            "cli-main-goal-captured-2",
            id = id.to_string(),
            a0 = (next_step_line(&goal, true)).to_string()
        ));
        return run::follow(std::sync::Arc::new(engine), out, id, false).await;
    }
    engine.shutdown().await;
    out.say(&bisa_core::text!(
        "cli-main-goal-captured-2",
        id = id.to_string(),
        a0 = (next_step_line(&goal, start)).to_string()
    ));
    say_minted(out, id, &secrets);
    if goal.listening.is_some() {
        out.say(&bisa_core::text!("cli-listening-no-node"));
    }
    out.json_value(json!({
        "goal": id.to_string(), "mode": goal.mode,
        "workflow": goal.workflow, "run": goal.run,
        "secrets": secrets,
    }));
    Ok(())
}

/// The secrets a goal's public hooks minted when it began to listen, said
/// where the person is told it listens — once: no later answer carries one.
fn say_minted(out: &Out, goal: GoalId, secrets: &[serde_json::Value]) {
    for line in listening::secret_lines(listening::HookHost::Goal, &goal.to_string(), secrets) {
        out.human(&line);
    }
}

/// Hold the embedded engine while the Workflow Agent takes its first turn on
/// a new goal, rendering its activity to stderr. Returns when the goal has
/// something waiting for the human (a question or the Adopt gate), when a
/// workflow has been proposed or a run started, or when the wake budget runs
/// out.
async fn wait_for_design(engine: &bisa_engine::Engine, id: GoalId, out: &Out) {
    use tokio::time::{Duration, Instant};

    let style = activity::Style::stderr();
    let mut events = engine.events();
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut poll = tokio::time::interval(Duration::from_millis(500));
    out.say(&bisa_core::text!("cli-main-workflow-agent-looking"));

    loop {
        tokio::select! {
            recv = events.recv() => {
                if let Ok(ev) = recv {
                    if let Some(line) = activity::engine_line(&style, &ev) {
                        eprintln!("{line}");
                    }
                }
            }
            _ = poll.tick() => {
                if !engine.inbox().is_empty() {
                    return; // a question or the Adopt gate is waiting on the human
                }
                let goal = match engine.workspace().get_goal(id) {
                    Ok(g) if g.workflow.is_some() || g.run.is_some() || g.is_closed() => return,
                    Ok(g) => g,
                    Err(_) => return,
                };
                // A wake that stopped without proposing, or never started,
                // is settled too: the goal says so, and `bisa design` is the
                // next move — holding the process past it helps nobody.
                let settled = engine
                    .workspace()
                    .journal(&bisa_core::Home::Goal { goal: id })
                    .ok()
                    .and_then(|journal| {
                        bisa_engine::guided::design_status(engine.inner(), &goal, None, &journal)
                    })
                    .is_some_and(|d| {
                        matches!(
                            d.status,
                            bisa_core::GuidanceStatus::Stalled | bisa_core::GuidanceStatus::Failed
                        )
                    });
                if settled {
                    out.say(&bisa_core::text!("cli-main-workflow-agent-stopped-without-proposal-bisa"));
                    return;
                }
                if Instant::now() >= deadline {
                    out.say(&bisa_core::text!("cli-main-workflow-agent-still-working-check-back"));
                    return;
                }
            }
        }
    }
}

/// Answer a question: an approval carrying back what the human said —
/// the options they picked, what they typed, or that they are not sure.
///
/// The three can arrive together, so they are one value rather than three
/// mutually exclusive flags: picking an option and qualifying it in a sentence
/// is an ordinary answer, not a contradiction.
/// `--step`, as a step id; a bad one is refused before anything is asked.
fn parse_step(raw: Option<&str>) -> Result<Option<bisa_core::StepId>> {
    raw.map(|s| bisa_core::StepId::new(s).map_err(|e| anyhow::anyhow!("{e}")))
        .transpose()
}

async fn answer(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    text: Option<&str>,
    options: &[String],
    unsure: bool,
    step: Option<bisa_core::StepId>,
) -> Result<()> {
    let answer = Answer {
        selected: options.to_vec(),
        text: text.map(str::to_string),
        unsure,
    };
    // Refused here rather than journaled as an empty answer: `answer <id> ""`
    // used to record `Some("")`, a fact that says a question was answered and
    // carries nothing that answers it.
    if answer.is_empty() {
        anyhow::bail!(bisa_core::text!(
            "cli-main-say-something-pass-your-answer-as"
        ));
    }
    // A gate id is accepted too — resolve it to its home first.
    if let Some(client) = ctx.node_client().await {
        let (home, gate) = resolve_answer_target_via_node(ctx, &client, id).await?;
        let v = client
            .post(
                &target::decide_route(&home),
                json!({"approve": true, "answer": answer, "gate": gate, "step": step}),
            )
            .await?;
        let outcome: DecideOutcome = serde_json::from_value(v).context(bisa_core::text!(
            "cli-main-unexpected-decide-payload-from-node"
        ))?;
        say_answered(out, &outcome);
        return Ok(());
    }
    let ws = ctx.workspace()?;
    let home = target::Target::resolve(&ws, id)?.home(&ws)?;
    let (engine, _) = ctx.engine().await?;
    let outcome =
        engine.decide_durable(&home, true, None, Some(&answer), None, None, step.as_ref());
    engine.shutdown().await;
    say_answered(out, &outcome?);
    Ok(())
}

/// What an answer settled: the home it was filed at and where that stands.
fn say_answered(out: &Out, outcome: &DecideOutcome) {
    out.say(&bisa_core::text!(
        "cli-main-answered-home",
        home = outcome.home.to_string(),
        status = outcome.status.as_str().to_string()
    ));
    out.json_value(json!({
        "home": outcome.home,
        "answered": true,
        "status": outcome.status.as_str(),
    }));
}

/// Accept a goal's id, a run's, or a live gate's id for `answer`: the home
/// it is decided through, and the gate when one was named.
async fn resolve_answer_target_via_node(
    ctx: &Ctx,
    client: &client::NodeClient,
    id: &str,
) -> Result<(bisa_core::Home, Option<String>)> {
    let ws = ctx.workspace()?;
    if let Ok(found) = target::Target::resolve(&ws, id) {
        return Ok((found.home(&ws)?, None));
    }
    let rows = client.get("/inbox").await?;
    for row in rows["rows"].as_array().cloned().unwrap_or_default() {
        for action in row["needs_action"].as_array().cloned().unwrap_or_default() {
            if action["gate_id"].as_str() == Some(id) {
                let home: bisa_core::Home = serde_json::from_value(action["home"].clone())
                    .context(bisa_core::text!(
                        "cli-main-unexpected-decide-payload-from-node"
                    ))?;
                return Ok((home, Some(id.to_string())));
            }
        }
    }
    bail!(bisa_core::text!(
        "cli-main-neither-goal-id-nor-pending-gate",
        id = format!("{id:?}")
    ))
}

/// The word a fate is asked for as, on the command line.
fn parse_fate(word: &str) -> Result<bisa_engine::retire::Fate> {
    use bisa_engine::retire::Fate;
    Ok(match word {
        "keep" => Fate::Keep,
        "archive" => Fate::Archive,
        "delete" => Fate::Delete,
        other => bail!(bisa_core::text!(
            "cli-main-not-fate-keep-archive-delete",
            other = format!("{other:?}")
        )),
    })
}

/// `archive <kind> <id> [--undo]`: put away, or take back out.
async fn archive(ctx: &Ctx, out: &Out, kind: &str, id: &str, archived: bool) -> Result<()> {
    let path = match kind {
        "goal" => format!("/goals/{}/archive", parse_goal_id(id)?),
        "workflow" => format!(
            "/workflows/{}/archive",
            bisa_core::WorkflowId::from_str(id).with_context(|| bisa_core::text!(
                "cli-main-not-workflow-id",
                id = format!("{id:?}")
            ))?
        ),
        "project" => format!("/projects/{}/archive", projects::parse_project(id)?),
        other => bail!(bisa_core::text!(
            "cli-main-not-kind-goal-workflow-project",
            other = format!("{other:?}")
        )),
    };
    let verb = if archived { "archived" } else { "unarchived" };
    if let Some(client) = ctx.node_client().await {
        let v = client.post(&path, json!({"archived": archived})).await?;
        out.human(&format!("{kind} {id} {verb}"));
        out.json_value(v);
        return Ok(());
    }
    let (engine, _) = ctx.engine().await?;
    let v = match kind {
        "goal" => {
            let goal = parse_goal_id(id)?;
            if archived {
                engine
                    .retire_goal(
                        goal,
                        bisa_engine::retire::GoalPlan {
                            goal: bisa_engine::retire::Fate::Archive,
                            projects: bisa_engine::retire::Fate::Keep,
                            tree: false,
                        },
                    )
                    .await?;
            } else {
                engine.unarchive_goal(goal)?;
            }
            json!({"goal": engine.workspace().get_goal(goal)?})
        }
        "workflow" => {
            json!({"workflow": engine.archive_workflow(bisa_core::WorkflowId::from_str(id)?, archived)?})
        }
        _ => json!({"project": engine.archive_project(projects::parse_project(id)?, archived)?}),
    };
    engine.shutdown().await;
    out.human(&format!("{kind} {id} {verb}"));
    out.json_value(v);
    Ok(())
}

/// `rm <goal> [--projects …] [--tree]`: the goal deleted on a plan.
async fn rm_goal(ctx: &Ctx, out: &Out, id: &str, projects: &str, tree: bool) -> Result<()> {
    let goal = parse_goal_id(id)?;
    let plan = bisa_engine::retire::GoalPlan {
        goal: bisa_engine::retire::Fate::Delete,
        projects: parse_fate(projects)?,
        tree,
    };
    let retired = if let Some(client) = ctx.node_client().await {
        client
            .post(
                &format!("/goals/{goal}/retire"),
                serde_json::to_value(plan)?,
            )
            .await?["retired"]
            .clone()
    } else {
        let (engine, _) = ctx.engine().await?;
        let done = engine.retire_goal(goal, plan).await?;
        engine.shutdown().await;
        serde_json::to_value(done)?
    };
    let unsettled = retired["unsettled_sessions"].as_u64().unwrap_or(0);
    out.say(&bisa_core::text!(
        "cli-main-goal-deleted-session-s-stopped-project",
        goal = goal.to_string(),
        a0 = (retired["stopped_sessions"]).to_string(),
        a1 = (if unsettled > 0 {
            bisa_i18n::say(&bisa_core::text!(
                "cli-main-still-ending-when-wait-ran-out",
                unsettled = unsettled.to_string()
            ))
        } else {
            String::new()
        })
        .to_string(),
        a2 = (retired["projects"].as_array().map(|p| p.len()).unwrap_or(0)).to_string(),
        a3 = (match projects {
            "archive" => "archived",
            "delete" => "deleted",
            _ => "kept",
        })
        .to_string()
    ));
    out.json_value(json!({"goal": goal.to_string(), "retired": retired}));
    Ok(())
}

/// Ask the Workflow Agent to design the goal's workflow again. The wake runs
/// inside a node, so this needs one running: an embedded engine would exit
/// with the design half done.
async fn design(ctx: &Ctx, out: &Out, goal: &str) -> Result<()> {
    let goal = parse_goal_id(goal)?;
    let Some(client) = ctx.node_client().await else {
        bail!(bisa_core::text!(
            "cli-main-bisa-design-needs-running-node-start"
        ));
    };
    let v = client
        .post(&format!("/goals/{goal}/design"), json!({}))
        .await?;
    out.say(&bisa_core::text!(
        "cli-main-asked-workflow-agent-design-goal-s",
        goal = goal.to_string(),
        a0 = (design_line(&v["guidance"]["design"])
            .map(|l| format!(" — {l}"))
            .unwrap_or_default())
        .to_string()
    ));
    out.json_value(v);
    Ok(())
}

/// One line for where the Workflow Agent stands, from a `design` status
/// object as the node sends it; nothing when there is none.
fn design_line(design: &serde_json::Value) -> Option<String> {
    let status = design["status"].as_str()?;
    let phase = design["phase"].as_str().unwrap_or("design");
    let detail = design["detail"]
        .as_str()
        .map(|d| format!(" — {d}"))
        .unwrap_or_default();
    let live = if design["live"].as_bool().unwrap_or(false) {
        " (live)"
    } else {
        ""
    };
    Some(format!("{phase}: {status}{live}{detail}"))
}

/// Replace the not-yet-started steps of a goal's running workflow. Gated
/// nowhere: the person is the one editing. Through the engine, because the
/// amendment is a run event and the run's effects are the engine's to run.
async fn amend(ctx: &Ctx, out: &Out, goal: &str, from: &str) -> Result<()> {
    let goal = parse_goal_id(goal)?;
    let draft = workflow::read_definition(&ctx.workspace()?, from)?;
    let run: WorkflowRun = if let Some(client) = ctx.node_client().await {
        let v = client
            .post(
                &format!("/goals/{goal}/amend"),
                json!({"workflow": serde_json::to_value(&draft)?}),
            )
            .await?;
        serde_json::from_value(v["run"].clone()).context(bisa_core::text!(
            "cli-main-unexpected-run-payload-from-node"
        ))?
    } else {
        let (engine, _) = ctx.engine().await?;
        let r = engine.amend_run(goal, draft);
        engine.shutdown().await;
        r?
    };
    out.say(&bisa_core::text!(
        "cli-main-run-amended-revision",
        a0 = (run.id).to_string(),
        a1 = (run.revision).to_string(),
        a2 = (run.status().as_str()).to_string()
    ));
    out.json_value(json!({"goal": goal.to_string(), "run": run}));
    Ok(())
}

/// The inbox: the conversations that have ever wanted something from you,
/// kept once earned and marked read rather than deleted. Rows come from a
/// running daemon (which knows about live gates); without one, the durable
/// blocked-overlay view is the fallback — and that fallback is still the old
/// "open gate or unread" query, because the membership facts it would need
/// live behind the node's inbox route.
async fn inbox(ctx: &Ctx, out: &Out) -> Result<()> {
    let rows: Vec<serde_json::Value> = if let Some(client) = ctx.node_client().await {
        let v = client.get("/inbox").await?;
        v["rows"].as_array().cloned().unwrap_or_default()
    } else {
        let ws = ctx.workspace()?;
        let mut rows = Vec::new();
        let unread: std::collections::HashMap<String, u64> =
            ws.unread_counts()?.into_iter().collect();
        for goal in ws.list_goals(None)? {
            let key = goal.id.to_string();
            let n = unread.get(&key).copied().unwrap_or(0);
            let run = ws.get_current_run(goal.id)?;
            let status = goal.status(run.as_ref());
            // One rule for "whose move": the core's holder projection —
            // the same word the node and the desktop print.
            let owed = goal.holder(run.as_ref(), false) == bisa_core::Holder::You;
            if !owed && n == 0 {
                continue;
            }
            rows.push(json!({
                "key": key,
                "kind": "goal",
                "title": goal.title.clone().unwrap_or_else(|| goal.statement.chars().take(80).collect()),
                "unread_count": n,
                "status": status,
                "needs_action": [],
            }));
        }
        for (scope, n) in unread {
            if rows.iter().any(|r| r["key"] == json!(scope)) || n == 0 {
                continue;
            }
            let title = bisa_core::ChannelId::new(&scope)
                .ok()
                .and_then(|c| ws.get_channel(&c).ok())
                .map(|c| c.name)
                .unwrap_or_else(|| scope.clone());
            rows.push(json!({
                "key": scope, "kind": "channel", "title": title,
                "unread_count": n, "needs_action": [],
            }));
        }
        rows
    };
    render_inbox(out, &rows);
    Ok(())
}

/// Gate decision. A running daemon owns the live gates (waiters resolve
/// immediately); otherwise the durable mirror applies the decision.
///
/// `inputs` matter for one gate: adopting a proposed workflow starts its run
/// with them.
async fn approve(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    approve: bool,
    rationale: Option<&str>,
    inputs: inputs::Typed,
    step: Option<bisa_core::StepId>,
) -> Result<()> {
    let ws = ctx.workspace()?;
    let home = target::Target::resolve(&ws, id)?.home(&ws)?;
    let listened = listens(&ws, &home);
    let inputs = inputs.read_for_home(&ws, &home);
    drop(ws);
    if let Some(client) = ctx.node_client().await {
        let outcome = client
            .post(
                &target::decide_route(&home),
                json!({"approve": approve, "rationale": rationale, "inputs": inputs, "step": step}),
            )
            .await?;
        let outcome: DecideOutcome = serde_json::from_value(outcome).context(bisa_core::text!(
            "cli-main-unexpected-decide-payload-from-node"
        ))?;
        render_decision(out, &outcome);
        return Ok(());
    }
    let (engine, _) = ctx.engine().await?;
    let outcome = engine.decide_durable(
        &home,
        approve,
        rationale,
        None,
        Some(inputs),
        None,
        step.as_ref(),
    );
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            engine.shutdown().await;
            return Err(e.into());
        }
    };
    render_decision(out, &outcome);
    // An adoption that made the goal listen armed it where nothing hears.
    if !listened && listens(engine.workspace(), &home) {
        out.say(&bisa_core::text!("cli-listening-no-node"));
    }
    // An approval that started or moved a run would die with this process:
    // follow it to a settlement the way `bisa run` does — the goal's, or the
    // run of the workspace itself. (With a daemon the running engine
    // already carries it.)
    if approve && outcome.status.is_running() {
        let engine = std::sync::Arc::new(engine);
        return match home {
            bisa_core::Home::Goal { goal } => run::follow(engine, out, goal, true).await,
            bisa_core::Home::Run { run } => run::follow_run(engine, out, run, true).await,
        };
    }
    engine.shutdown().await;
    Ok(())
}

/// Whether `home` is a goal that listens.
fn listens(ws: &bisa_store::Workspace, home: &bisa_core::Home) -> bool {
    match home {
        bisa_core::Home::Goal { goal } => ws
            .get_goal(*goal)
            .is_ok_and(|goal| goal.listening.is_some()),
        bisa_core::Home::Run { .. } => false,
    }
}

async fn status(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    let id = match target::Target::resolve(&ws, id)? {
        target::Target::Goal(goal) => goal,
        target::Target::Run(run) => return run_status(ctx, out, &ws, run).await,
    };
    if let Some(client) = ctx.node_client().await {
        let v = client.get(&format!("/goals/{id}")).await?;
        let goal: bisa_core::Goal = serde_json::from_value(v["goal"].clone())
            .context(bisa_core::text!("cli-main-bad-goal-payload-from-node"))?;
        let run: Option<WorkflowRun> = serde_json::from_value(v["run"].clone())
            .context(bisa_core::text!("cli-main-bad-run-payload-from-node"))?;
        let items: Vec<WorkItemSpec> = serde_json::from_value(v["work_items"].clone()).context(
            bisa_core::text!("cli-main-bad-work-items-payload-from-node"),
        )?;
        let spent: bisa_core::goal::BudgetSpent = serde_json::from_value(v["spent"].clone())
            .context(bisa_core::text!("cli-main-bad-spent-payload-from-node"))?;
        // Who started the current run, as the node's own summary of it says.
        let started_by = run.as_ref().and_then(|current| {
            let id = current.id.to_string();
            listening::rows(&v["runs"])
                .iter()
                .find(|summary| summary["id"].as_str() == Some(id.as_str()))
                .map(run::StartedBy::read)
        });
        render_status(
            out,
            &GoalPage {
                hears: heard_by(&ws, &goal),
                goal: &goal,
                run: run.as_ref(),
                items: &items,
                spent: &spent,
                design: design_line(&v["guidance"]["design"]),
                started_by,
                heard: true,
            },
        );
        return Ok(());
    }
    let home = bisa_core::Home::Goal { goal: id };
    let goal = ws.get_goal(id)?;
    let run = ws.get_current_run(id)?;
    let items = ws.list_work_items(&home)?;
    let spent = ws.spent(&home)?;
    // No engine here, so nothing is known to be live: a fact that claims work
    // in progress reads as stalled, which is the honest answer from outside.
    let design = bisa_engine::guided::design_status_from_journal(
        &goal,
        run.as_ref(),
        &ws.journal(&home)?,
        false,
    )
    .and_then(|d| serde_json::to_value(d).ok())
    .and_then(|d| design_line(&d));
    render_status(
        out,
        &GoalPage {
            hears: heard_by(&ws, &goal),
            goal: &goal,
            run: run.as_ref(),
            items: &items,
            spent: &spent,
            design,
            started_by: run.as_ref().map(|run| run::StartedBy::of(&ws, run)),
            heard: false,
        },
    );
    Ok(())
}

/// What a listening goal hears: one line per start of its workflow that
/// begins on an event, summarised as it is armed — every input it reads
/// read. Nothing for a goal that does not listen.
fn heard_by(ws: &bisa_store::Workspace, goal: &bisa_core::Goal) -> Vec<String> {
    let (Some(listening), Some(workflow)) = (&goal.listening, goal.workflow) else {
        return Vec::new();
    };
    let Ok(workflow) = ws.get_workflow(workflow) else {
        return Vec::new();
    };
    bisa_node::dto::StartSummary::list(&workflow, Some(listening))
        .iter()
        .filter(|start| start.event != bisa_core::StartOn::Manual.as_str())
        .filter_map(|start| serde_json::to_value(start).ok())
        .map(|start| listening::start_line(&start))
        .collect()
}

async fn log(ctx: &Ctx, out: &Out, id: &str, limit: usize) -> Result<()> {
    let ws = ctx.workspace()?;
    let home = target::Target::resolve(&ws, id)?.home(&ws)?;
    let shown: Vec<bisa_core::JournalEvent> = if let Some(client) = ctx.node_client().await {
        let route = match home {
            bisa_core::Home::Goal { goal } => format!("/goals/{goal}/journal?limit={limit}"),
            bisa_core::Home::Run { run } => format!("/runs/{run}/journal?limit={limit}"),
        };
        let v = client.get(&route).await?;
        serde_json::from_value(v["events"].clone())
            .context(bisa_core::text!("cli-main-bad-journal-payload-from-node"))?
    } else {
        let events = ws.journal(&home)?;
        events.iter().rev().take(limit).rev().cloned().collect()
    };
    for e in &shown {
        out.human(&activity::journal_line(e));
    }
    out.json_value(json!({"home": home, "events": shown}));
    Ok(())
}

/// One run's status, by its id — a goal's or the workspace's: its workflow,
/// its steps, its own work items and what its home has spent.
async fn run_status(ctx: &Ctx, out: &Out, ws: &bisa_store::Workspace, id: RunId) -> Result<()> {
    let (run, holder, started_by) = if let Some(client) = ctx.node_client().await {
        let v = client.get(&format!("/runs/{id}")).await?;
        let run: WorkflowRun = serde_json::from_value(v["run"].clone())
            .context(bisa_core::text!("cli-main-bad-run-payload-from-node"))?;
        let holder: bisa_core::Holder = serde_json::from_value(v["holder"].clone())
            .context(bisa_core::text!("cli-main-bad-run-payload-from-node"))?;
        (run, holder, run::StartedBy::read(&v["summary"]))
    } else {
        let run = ws.get_run(id)?;
        // No engine here: the step-level marks below say which step is yours.
        let holder = run.holder(false);
        let started_by = run::StartedBy::of(ws, &run);
        (run, holder, started_by)
    };
    let home = run.home();
    let items: Vec<WorkItemSpec> = ws
        .list_work_items(&home)?
        .into_iter()
        .filter(|i| i.run == Some(run.id))
        .collect();
    let spent = ws.spent(&home)?;
    let mut lines = vec![format!(
        "{}  {}  ({})  [{}]",
        run.workflow.name,
        run.status().as_str(),
        holder.as_str(),
        match run.scope.goal() {
            Some(goal) => bisa_i18n::say(&bisa_core::text!(
                "cli-main-run-of-goal",
                goal = goal.to_string()
            )),
            None => bisa_i18n::say(&bisa_core::text!("cli-main-run-of-workspace")),
        }
    )];
    lines.push(started_by_line(&started_by));
    lines.extend(step_lines(&run));
    lines.extend(item_lines(&items));
    lines.push(spent_line(&spent));
    out.human(&lines.join("\n"));
    // A goal's status and a run's say where they stand under one key, so
    // what reads the one reads the other.
    out.json_value(json!({
        "status": run.status(), "run": run, "holder": holder,
        "started_by": started_by, "work_items": items, "spent": spent
    }));
    Ok(())
}

/// What `POST /goals/{id}/close` reads of a closure: the person's word on
/// why, or the goal that takes this one's place — the route's own keys,
/// never the closure as the journal writes it.
fn close_body(reason: &ClosureReason) -> serde_json::Value {
    match reason {
        ClosureReason::Superseded { by } => json!({ "superseded_by": by.to_string() }),
        ClosureReason::Abandoned { rationale } => json!({ "rationale": rationale }),
    }
}

/// The goal list, with or without a query.
///
/// One command rather than two: a full-text query and a tag filter answer the
/// same question at different resolutions, and splitting them would leave
/// `--tag` unable to reach goals at all.
fn search(ctx: &Ctx, out: &Out, query: Option<&str>, filter: &tags::TagFilterArgs) -> Result<()> {
    let ws = ctx.workspace()?;
    let admitted = filter.admitted(&ws, bisa_core::tags::TagEntity::Goal)?;
    let found: Vec<bisa_core::Goal> = match query {
        Some(q) => ws
            .search(q)?
            .into_iter()
            .filter_map(|id| ws.get_goal(id).ok())
            .collect(),
        None => ws.list_goals(None)?,
    };
    let mut rows = Vec::new();
    for goal in found {
        if !tags::keeps(&admitted, &goal.id.to_string()) {
            continue;
        }
        let run = ws.get_current_run(goal.id)?;
        let status = goal.status(run.as_ref());
        out.human(&format!(
            "{}  {:<8} {:<20} {}",
            goal.id,
            status.as_str(),
            tags::label(&goal.tags),
            goal.title.as_deref().unwrap_or(&goal.statement)
        ));
        rows.push(json!({
            "goal": goal.id.to_string(),
            "status": status,
            "workflow": goal.workflow,
            "statement": goal.statement,
            "tags": goal.tags,
        }));
    }
    if rows.is_empty() {
        out.say(&bisa_core::text!("cli-main-no-matches"));
    }
    out.json_value(json!({"query": query, "matches": rows}));
    Ok(())
}

async fn harness(ctx: &Ctx, out: &Out, cmd: HarnessCmd) -> Result<()> {
    let catalog = ctx.catalog();
    match cmd {
        HarnessCmd::List => {
            let listings = catalog.list().await;
            for l in &listings {
                let mark = if l.probe.available { "✓" } else { "✗" };
                let extra = l
                    .probe
                    .version
                    .clone()
                    .or_else(|| l.probe.reason.clone())
                    .or_else(|| l.install_hint.clone())
                    .unwrap_or_default();
                out.human(&format!(
                    "{mark} {:10} {:24} {}",
                    format!("{:?}", l.tier).to_lowercase(),
                    l.id,
                    extra
                ));
            }
            out.json_value(json!({"harnesses": listings}));
        }
        HarnessCmd::Probe { id } => {
            let listing = catalog.list().await.into_iter().find(|l| l.id == id);
            match listing {
                Some(l) => {
                    let detail = if l.probe.available {
                        format!(
                            "available{}",
                            l.probe
                                .version
                                .as_deref()
                                .map(|v| format!(" ({v})"))
                                .unwrap_or_default()
                        )
                    } else {
                        bisa_i18n::say(&bisa_core::text!(
                            "cli-main-unavailable",
                            a0 = (l.probe.reason.as_deref().unwrap_or_default()).to_string()
                        ))
                    };
                    out.human(&format!("{}: {detail}", l.id));
                    out.json_value(json!({"harness": l}));
                }
                None => bail!(bisa_core::text!(
                    "cli-main-unknown-harness-see-bisa-harness-list",
                    id = format!("{id:?}")
                )),
            }
        }
    }
    Ok(())
}

/// Refuse a non-loopback `--listen` unless the operator said so in as many
/// words.
///
/// The control plane answers to one bearer token kept on this machine, and a
/// public hook (`POST /hooks/{host}/{step}`) to its listener's secret: the
/// design of a local-first daemon, made for a port nothing outside reaches.
/// Binding `0.0.0.0` puts every route on the network at once, so the default
/// is to stop rather than to warn into a log nobody reads.
fn check_bind(addr: std::net::SocketAddr, allowed: bool) -> Result<()> {
    if addr.ip().is_loopback() || allowed {
        return Ok(());
    }
    anyhow::bail!(bisa_core::text!(
        "cli-main-refusing-listen-not-loopback-address-control",
        addr = addr.to_string()
    ))
}

async fn node(
    ctx: &Ctx,
    out: &Out,
    listen: Option<std::net::SocketAddr>,
    insecure_allow_remote: bool,
) -> Result<()> {
    if let Some(addr) = listen {
        check_bind(addr, insecure_allow_remote)?;
    }
    let engine = ctx.node_engine().await?;
    let socket = bisa_store::Paths::new(&ctx.data_dir).node_socket();
    out.say(&bisa_core::text!(
        "cli-main-bisa-node-listening-press-ctrl-c",
        a0 = (socket.display()).to_string(),
        a1 = (listen
            .map(|a| bisa_i18n::say(&bisa_core::text!("cli-main-http", a = a.to_string())))
            .unwrap_or_default())
        .to_string()
    ));
    out.json_value(json!({"socket": socket, "listen": listen}));
    // The collaboration pump beside the daemon: the relay pool on the
    // `sync.*` settings, the host's pump when they give it anyone to talk
    // through, the guest sessions — lent to the node as its doors.
    let pump = net::Pump::start(&engine, &ctx.data_dir).await?;
    out.say(&bisa_core::text!("cli-main-collaboration-pump-running"));
    // Which signal stopped the daemon, for the goodbye.
    let stopped: std::sync::Arc<std::sync::OnceLock<&'static str>> = Default::default();
    let collab: Option<bisa_node::collab::Collab> = Some(std::sync::Arc::clone(&pump) as _);
    // The attachment fetch is the host pump's, when one runs; without one
    // the fetch route says "there is nobody to ask".
    let fetch_attachment: Option<bisa_node::AttachmentFetcher> = {
        let pump = std::sync::Arc::clone(&pump);
        Some(std::sync::Arc::new(move |sha256: String| {
            let pump = std::sync::Arc::clone(&pump);
            Box::pin(async move {
                match pump.attachment_fetcher().await {
                    Some(f) => Ok(f.fetch(&sha256).await),
                    None => Err(bisa_i18n::say(&bisa_core::text!(
                        "cli-main-node-runs-no-host-pump"
                    ))),
                }
            })
        }))
    };
    bisa_node::serve(
        engine,
        bisa_node::NodeConfig {
            socket,
            http: listen,
            data_dir: ctx.data_dir.clone(),
            collab,
            fetch_attachment,
            // A token handed in (the desktop gives its sidecar one) wins and
            // is recorded in run/token; otherwise the node reads or mints it.
            token: std::env::var("BISA_API_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty()),
            // A2A expose is opt-in via <data-dir>/a2a.toml (public_base_url,
            // optional workflow / auto_start / skills).
            a2a: {
                let path = ctx.data_dir.join("a2a.toml");
                match std::fs::read_to_string(&path) {
                    Ok(body) => match toml::from_str::<bisa_node::A2aExposeConfig>(&body) {
                        Ok(cfg) => {
                            eprintln!(
                                "{}",
                                bisa_i18n::say(&bisa_core::text!(
                                    "cli-main-a2a-exposed-a2a",
                                    a0 = (cfg.public_base_url).to_string()
                                ))
                            );
                            Some(cfg)
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "bisa_cli",
                                path = %path.display(),
                                "a2a.toml is invalid ({e}); A2A disabled"
                            );
                            None
                        }
                    },
                    Err(_) => None,
                }
            },
        },
        {
            let stopped = std::sync::Arc::clone(&stopped);
            async move {
                let reason = stop_signal().await;
                tracing::info!(target: "bisa_cli", reason, "stop signal received");
                // The first reason stands; a second signal changes nothing.
                let _first_reason_stands = stopped.set(reason);
            }
        },
    )
    .await?;
    pump.stop().await;
    ctx.log.goodbye(stopped.get().copied().unwrap_or("stopped"));
    Ok(())
}

/// The daemon's stop: ctrl-c (`SIGINT`) or `SIGTERM` — what launchd, a
/// supervisor and the desktop shell send — each named for the goodbye.
async fn stop_signal() -> &'static str {
    #[cfg(unix)]
    {
        let mut term =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(term) => term,
                Err(e) => {
                    tracing::warn!(target: "bisa_cli", "SIGTERM is not listened for: {e}");
                    if let Err(e) = tokio::signal::ctrl_c().await {
                        tracing::warn!(target: "bisa_cli", "Ctrl-C is not listened for: {e}");
                    }
                    return "interrupted";
                }
            };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => "interrupted",
            _ = term.recv() => "terminated",
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!(target: "bisa_cli", "Ctrl-C is not listened for: {e}");
        }
        "interrupted"
    }
}

/// Close a goal. Through the engine, never the store alone: the effects —
/// cancelling the run and its items, releasing workstreams — are the engine's
/// to run.
async fn close(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    rationale: Option<String>,
    superseded_by: Option<String>,
) -> Result<()> {
    let id = parse_goal_id(id)?;
    let reason = match superseded_by {
        Some(by) => ClosureReason::Superseded {
            by: parse_goal_id(&by)?,
        },
        None => ClosureReason::Abandoned {
            rationale: rationale.filter(|r| !r.trim().is_empty()),
        },
    };
    if let Some(client) = ctx.node_client().await {
        let v = client
            .post(&format!("/goals/{id}/close"), close_body(&reason))
            .await?;
        out.say(&bisa_core::text!(
            "cli-main-goal-closed-run-cancelled-work-items",
            id = id.to_string(),
            a0 = (reason.as_str()).to_string()
        ));
        out.json_value(json!({"goal": id.to_string(), "closed": v["goal"]["closed"]}));
        return Ok(());
    }
    let (engine, _) = ctx.engine().await?;
    let closed = engine.close_goal(id, reason);
    engine.shutdown().await;
    let goal = closed?;
    out.say(&bisa_core::text!(
        "cli-main-goal-closed-run-cancelled-work-items",
        id = id.to_string(),
        a0 = (goal
            .closed
            .as_ref()
            .map(|c| c.reason.as_str())
            .unwrap_or("closed"))
        .to_string()
    ));
    out.json_value(json!({"goal": id.to_string(), "closed": goal.closed}));
    Ok(())
}

async fn governance(ctx: &Ctx, out: &Out, cmd: GovernanceCmd) -> Result<()> {
    use bisa_store::GatePolicy;
    let ws = ctx.workspace()?;
    let render = |out: &Out, gov: &bisa_store::Governance| {
        let show = |p: &GatePolicy| match p {
            GatePolicy::Owner => "owner".to_string(),
            GatePolicy::Admins => "admins".to_string(),
            GatePolicy::Members => "members".to_string(),
            GatePolicy::Listed(entries) => entries.join(","),
        };
        out.say(&bisa_core::text!(
            "cli-main-approval-escalation-publish",
            a0 = (show(&gov.approval)).to_string(),
            a1 = (show(&gov.escalation)).to_string(),
            a2 = (show(&gov.publish)).to_string()
        ));
        out.json_value(json!({"governance": gov}));
    };
    match cmd {
        GovernanceCmd::Show => render(out, &ws.governance()?),
        GovernanceCmd::Set { gate, policy } => {
            let gate = Gate::from_str(gate.trim()).map_err(|_| {
                anyhow::anyhow!(bisa_core::text!(
                    "cli-main-unknown-gate-approval-escalation-publish",
                    gate = format!("{gate:?}")
                ))
            })?;
            let policy = match policy.as_str() {
                "owner" => GatePolicy::Owner,
                "admins" => GatePolicy::Admins,
                "members" => GatePolicy::Members,
                // Entries are pubkey hex or `team:<id>` (expanded to the
                // team's humans when a decision is checked).
                list => GatePolicy::Listed(
                    list.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                ),
            };
            // Through the node when one runs — one engine holds a workspace,
            // and who may decide a gate is read by the engine that asks —
            // into the store otherwise.
            let gov = match ctx.node_client().await {
                Some(node) => {
                    let answer = node
                        .put("/governance", json!({ gate.as_str(): policy }))
                        .await?;
                    serde_json::from_value(answer["governance"].clone()).context(
                        bisa_core::text!("cli-main-unexpected-governance-payload-from-node"),
                    )?
                }
                None => ws.set_gate_policy(gate, policy)?,
            };
            render(out, &gov);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared renderers (same output whether data came from the store or the node)
// ---------------------------------------------------------------------------

/// Everything a goal's page prints, gathered from the node or from the store.
struct GoalPage<'a> {
    goal: &'a bisa_core::Goal,
    run: Option<&'a WorkflowRun>,
    items: &'a [WorkItemSpec],
    spent: &'a bisa_core::goal::BudgetSpent,
    /// Where the Workflow Agent stands, when it has a standing.
    design: Option<String>,
    /// What the goal hears while it listens: one line per event start.
    hears: Vec<String>,
    /// Who started its current run.
    started_by: Option<run::StartedBy>,
    /// Whether a node is there to hear what the goal listens for.
    heard: bool,
}

/// A goal's listening, on its page: where it stands — since when, with what,
/// paused and why — what it hears, and the verb that comes next. `heard` is
/// whether a node is there to hear it; without one its events are heard once
/// one runs.
fn listening_page(goal: &bisa_core::Goal, hears: &[String], heard: bool) -> Vec<String> {
    let Some(standing) = &goal.listening else {
        return Vec::new();
    };
    let mut lines = listening::standing_lines(Some(standing));
    lines.extend(hears.iter().map(|line| format!("  {line}")));
    if standing.is_paused() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-main-listen-again",
            id = goal.id.to_string()
        )));
        return lines;
    }
    lines.push(bisa_i18n::say(&bisa_core::text!(
        "cli-main-listeners-of-goal",
        id = goal.id.to_string()
    )));
    if !heard {
        lines.push(format!(
            "  {}",
            bisa_i18n::say(&bisa_core::text!("cli-listening-no-node"))
        ));
    }
    lines
}

fn render_status(out: &Out, page: &GoalPage<'_>) {
    let GoalPage {
        goal,
        run,
        items,
        spent,
        ..
    } = *page;
    let status = goal.status(run);
    // `owed` is false here: the durable gates live on the daemon, and the
    // step-level marks below already say which step is yours.
    let holder = goal.holder(run, false);
    let mut lines = vec![format!(
        "{}  {}  ({}){}",
        goal.title.as_deref().unwrap_or(&goal.statement),
        status.as_str(),
        holder.as_str(),
        goal.closed
            .as_ref()
            .map(|c| format!("  [{}]", c.reason.as_str()))
            .unwrap_or_default()
    )];
    match (goal.workflow, run) {
        (Some(wf), None) => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-main-workflow-no-run-yet",
            wf = wf.to_string()
        ))),
        (_, Some(run)) => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-main-workflow-rev-run",
            a0 = (run.workflow.id).to_string(),
            a1 = (run.workflow.name).to_string(),
            a2 = (run.workflow.revision).to_string(),
            a3 = (run.id).to_string(),
            a4 = (run.status().as_str()).to_string(),
            a5 = (run
                .outcome
                .map(|o| format!(", {}", o.as_str()))
                .unwrap_or_default())
            .to_string()
        ))),
        (None, None) => lines.push(match goal.mode {
            bisa_core::GoalMode::Auto => bisa_i18n::say(&bisa_core::text!(
                "cli-main-workflow-none-yet-workflow-agent-designs"
            )),
            bisa_core::GoalMode::Guided => bisa_i18n::say(&bisa_core::text!(
                "cli-main-workflow-none-yet-workflow-agent-proposes"
            )),
            bisa_core::GoalMode::Manual => bisa_i18n::say(&bisa_core::text!(
                "cli-main-workflow-none-design-bisa-workflow-use"
            )),
        }),
    }
    if let Some(started_by) = &page.started_by {
        lines.push(started_by_line(started_by));
    }
    lines.extend(listening_page(goal, &page.hears, page.heard));
    if let Some(design) = &page.design {
        lines.push(format!("design:    {design}"));
    }
    if !goal.assignees.is_empty() {
        let who: Vec<String> = goal.assignees.iter().map(|a| a.to_string()).collect();
        lines.push(format!("assignees: {}", who.join(", ")));
    }
    if let Some(run) = run {
        lines.extend(step_lines(run));
    }
    lines.extend(item_lines(items));
    lines.push(spent_line(spent));
    out.human(&lines.join("\n"));
    out.json_value(json!({
        "goal": goal, "status": status, "run": run, "work_items": items, "spent": spent
    }));
}

/// Who started a run, as a line of its page.
fn started_by_line(started_by: &run::StartedBy) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-main-run-started-by",
        by = started_by.words()
    ))
}

/// What a step's state adds to its line: the branches a gateway chose, the
/// boundary event that diverted it — or its error.
fn step_detail(record: &bisa_core::StepRecord) -> String {
    match &record.state {
        bisa_core::StepState::Done { branches } if !branches.is_empty() => format!(
            "  → {}",
            branches
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(" · ")
        ),
        bisa_core::StepState::Diverted { by } => bisa_i18n::say(&bisa_core::text!(
            "cli-main-step-diverted-by",
            by = by.to_string()
        )),
        _ => record
            .error
            .as_deref()
            .map(|e| format!("  — {}", e.chars().take(60).collect::<String>()))
            .unwrap_or_default(),
    }
}

/// A run's steps, one line each with its state's mark and what the state
/// adds: the branches chosen, the boundary that diverted it, its error.
fn step_lines(run: &WorkflowRun) -> Vec<String> {
    let mut lines = vec!["steps:".to_string()];
    for step in &run.workflow.steps {
        let record = run.steps.get(&step.id);
        let mark = match record.map(|r| &r.state) {
            None | Some(bisa_core::StepState::Pending) => "·",
            Some(bisa_core::StepState::Running) => "▸",
            Some(bisa_core::StepState::Waiting) => "⏸",
            Some(bisa_core::StepState::Done { .. }) => "✓",
            Some(bisa_core::StepState::Skipped) => "–",
            Some(bisa_core::StepState::Failed) => "✗",
            Some(bisa_core::StepState::Cancelled) => "⊘",
            Some(bisa_core::StepState::Diverted { .. }) => "↪",
        };
        let detail = record.map(step_detail).unwrap_or_default();
        lines.push(format!(
            "  {mark} {:<20} {:<9} {}{detail}",
            step.id,
            activity::step_kind_label(&step.kind),
            step.name
        ));
    }
    lines
}

/// Work items, one line each; nothing when there are none.
fn item_lines(items: &[WorkItemSpec]) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!("cli-main-work-items"))];
    for w in items {
        lines.push(format!(
            "  {}  {}  [{}]{}",
            w.id,
            w.instructions.chars().take(60).collect::<String>(),
            activity::workitem_state_label(&w.state),
            w.step
                .as_ref()
                .map(|s| format!("  step {s}"))
                .unwrap_or_default()
        ));
    }
    lines
}

fn spent_line(spent: &bisa_core::goal::BudgetSpent) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-main-spent-tokens-s-wall",
        a0 = (spent.tokens).to_string(),
        a1 = (spent.usd_cents).to_string(),
        a2 = (spent.wall_clock_secs).to_string()
    ))
}

/// Conversation rows, questions first. A question shows how to answer it;
/// a gate shows how to decide it.
fn render_inbox(out: &Out, rows: &[serde_json::Value]) {
    if rows.is_empty() {
        out.say(&bisa_core::text!(
            "cli-main-inbox-empty-nothing-waiting-you"
        ));
    }
    for r in rows {
        let key = r["key"].as_str().unwrap_or("");
        let title = r["title"].as_str().unwrap_or("");
        let unread = r["unread_count"].as_u64().unwrap_or(0);
        let badge = if unread > 0 {
            format!("  {unread} unread")
        } else {
            String::new()
        };
        // Rows are kept once earned, so the list holds conversations you have
        // already dealt with. Without a mark for read the terminal has no way
        // to say so, and every row looks equally owed.
        let mark = if r["read"].as_bool().unwrap_or(false) {
            "·"
        } else {
            "●"
        };
        out.human(&format!(
            "{mark} {key}  {}  {title}{badge}",
            r["kind"].as_str().unwrap_or("")
        ));
        if r["handled"].as_bool().unwrap_or(false) {
            let d = &r["decided"];
            out.human(&format!(
                "   ✓ {} {}",
                d["gate_kind"].as_str().unwrap_or("gate"),
                if d["approve"].as_bool().unwrap_or(false) {
                    "approved"
                } else {
                    "declined"
                }
            ));
        }
        for a in r["needs_action"].as_array().cloned().unwrap_or_default() {
            let question = a["question"].as_str().unwrap_or("");
            if a["expects"]["kind"].as_str() == Some("answer") {
                let offered: Vec<String> = a["expects"]["options"]
                    .as_array()
                    .map(|os| {
                        os.iter()
                            .map(|o| {
                                format!(
                                    "[{}] {}{}",
                                    o["id"].as_str().unwrap_or(""),
                                    o["label"].as_str().unwrap_or(""),
                                    if o["recommended"].as_bool().unwrap_or(false) {
                                        " (recommended)"
                                    } else {
                                        ""
                                    }
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                // The two escape hatches are always shown, whether or not the
                // asker offered options: they belong to the platform, so a
                // narrower question cannot take them away.
                let hint = if offered.is_empty() {
                    format!("     → bisa answer {key} \"…\"  |  --unsure")
                } else {
                    format!(
                        "     {}\n     → bisa answer {key} -o <id> [\"…\"]  |  --unsure",
                        offered.join("  ")
                    )
                };
                out.human(&format!("   ? {question}\n{hint}"));
            } else {
                let inputs_hint =
                    if a["gate_kind"].as_str() == Some("approval") && a["step"].is_null() {
                        " [--input k=v]"
                    } else {
                        ""
                    };
                out.human(&format!(
                    "   ⏸ {question}\n     → bisa approve {key} [--no]{inputs_hint}"
                ));
            }
        }
        if let Some(status) = r["status"].as_str() {
            if status == "waiting" && r["needs_action"].as_array().is_none_or(|a| a.is_empty()) {
                out.human(&format!("   ⏸ waiting — see: bisa status {key}"));
            }
        }
        if let Some(rep) = r["representative"].as_object() {
            out.human(&format!(
                "   ✉ {}",
                rep.get("snippet").and_then(|s| s.as_str()).unwrap_or("")
            ));
        }
        // What happened to it, the unread ones only: a notice is read here
        // and opened there, and the row says how many the terminal did not print.
        let unread_notices = r["unread_notices"].as_u64().unwrap_or(0) as usize;
        let notices = r["notices"].as_array().cloned().unwrap_or_default();
        for n in notices.iter().take(unread_notices) {
            out.human(&format!(
                "   • {}{}",
                n["notice"].as_str().unwrap_or("").replace('_', " "),
                n["title"]
                    .as_str()
                    .map(|t| format!(" — {t}"))
                    .unwrap_or_default()
            ));
        }
        if notices.len() > unread_notices && unread_notices > 0 {
            out.say(&bisa_core::text!(
                "cli-main-you-have-read",
                a0 = (notices.len() - unread_notices).to_string()
            ));
        }
    }
    out.json_value(json!({"rows": rows}));
}

fn render_decision(out: &Out, outcome: &DecideOutcome) {
    let decided = if outcome.approve {
        "approved"
    } else {
        "rejected"
    };
    let home = outcome.home;
    let status = outcome.status.as_str();
    // A goal whose run the approval started is followed with `bisa run`;
    // a run of the workspace is followed by its own id.
    let follow = match home {
        bisa_core::Home::Goal { goal } if outcome.approve && outcome.status.is_running() => {
            bisa_i18n::say(&bisa_core::text!(
                "cli-main-follow-with-bisa-run-watch",
                id = goal.to_string()
            ))
        }
        _ => String::new(),
    };
    match outcome.gate {
        Gate::Approval => out.say(&bisa_core::text!(
            "cli-main-approval-home",
            decided = decided.to_string(),
            home = home.to_string(),
            status = status.to_string(),
            a0 = follow
        )),
        Gate::Escalation => out.say(&bisa_core::text!(
            "cli-main-escalation-home",
            decided = decided.to_string(),
            home = home.to_string(),
            status = status.to_string()
        )),
        Gate::Publish => out.say(&bisa_core::text!(
            "cli-main-publish",
            decided = decided.to_string()
        )),
    }
    // An adoption that made its goal listen minted its public hooks'
    // secrets: they are shown here, once.
    let secrets: Vec<serde_json::Value> = outcome
        .secrets
        .iter()
        .filter_map(|secret| serde_json::to_value(secret).ok())
        .collect();
    if let bisa_core::Home::Goal { goal } = home {
        say_minted(out, goal, &secrets);
    }
    out.json_value(json!({
        "home": home,
        "gate": outcome.gate,
        "approve": outcome.approve,
        "status": status,
        "secrets": secrets,
    }));
}

/// `bisa doctor`: the setup gate's five checks, in the terminal (16 — The
/// setup gate). Through the node when one runs, else the embedded engine —
/// the same `Readiness` either way. Nothing is installed or run here; the
/// official line for this platform is printed to copy.
async fn doctor(ctx: &Ctx, out: &Out) -> Result<()> {
    let readiness = if let Some(client) = ctx.node_client().await {
        client.get("/readiness").await?
    } else {
        let (engine, _) = ctx.engine().await?;
        let r = engine.readiness().await;
        engine.shutdown().await;
        serde_json::to_value(r)?
    };
    let platform = match std::env::consts::OS {
        "macos" => "mac_os",
        "windows" => "windows",
        _ => "linux",
    };
    let ready = readiness["ready"].as_bool().unwrap_or(false);
    for check in readiness["checks"].as_array().into_iter().flatten() {
        let mark = match check["state"].as_str().unwrap_or("") {
            "ready" => "✓",
            "missing" => "✗",
            _ => "!",
        };
        out.human(&format!(
            "{mark} {} — {}",
            check["title"].as_str().unwrap_or(""),
            check["detail"].as_str().unwrap_or("")
        ));
        if check["state"] != "ready" {
            if let Some(hint) = check.get("hint").filter(|h| h.is_object()) {
                let line = hint["commands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|c| c["platform"] == platform)
                    .and_then(|c| c["command"].as_str());
                if let Some(line) = line {
                    out.human(&format!("    install: {line}"));
                }
                if let Some(url) = hint["url"].as_str() {
                    out.human(&format!("    docs: {url}"));
                }
            }
            for fix in check["fixes"].as_array().into_iter().flatten() {
                if let Some(label) = fix["label"].as_str() {
                    out.say(&bisa_core::text!(
                        "cli-main-fix-desktop",
                        label = label.to_string()
                    ));
                }
            }
        }
    }
    out.say(&if ready {
        bisa_core::text!("cli-main-everything-platform-needs-here")
    } else {
        bisa_core::text!("cli-main-something-missing-desktop-s-setup-gate")
    });
    out.json_value(readiness);
    if !ready {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The close route reads its own keys: a closure sent as the journal
    /// writes it named the goal that takes this one's place under a key the
    /// route never read, and the goal was closed as given up on.
    #[test]
    fn a_closure_is_sent_under_the_keys_the_route_reads() {
        let by = bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(7, 7));
        assert_eq!(
            super::close_body(&bisa_core::ClosureReason::Superseded { by }),
            serde_json::json!({ "superseded_by": by.to_string() })
        );
        assert_eq!(
            super::close_body(&bisa_core::ClosureReason::Abandoned {
                rationale: Some("the shelf is up".into())
            }),
            serde_json::json!({ "rationale": "the shelf is up" })
        );
        assert_eq!(
            super::close_body(&bisa_core::ClosureReason::Abandoned { rationale: None }),
            serde_json::json!({ "rationale": null })
        );
    }

    use super::*;
    use bisa_core::{Branch, StepRecord, StepState};

    /// A goal, as its snapshot reads, standing as `listening` says.
    fn goal(listening: serde_json::Value) -> bisa_core::Goal {
        serde_json::from_value(json!({
            "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "statement": "Answer every ticket",
            "author": "cd".repeat(32),
            "listening": listening,
            "origin": {"origin": "captured"},
            "revision": 1,
            "created_at": 0,
        }))
        .expect("a goal")
    }

    /// A line the catalog has: a miss renders as the message's id.
    fn sentence(line: &str) {
        assert!(
            !line.is_empty() && !line.contains("cli-"),
            "a line the catalog does not say: {line:?}"
        );
    }

    #[test]
    fn a_listening_goal_s_page_says_since_when_what_it_hears_and_who_hears_it() {
        let quiet = goal(json!(null));
        assert!(listening_page(&quiet, &[], true).is_empty());

        let listens = goal(json!({"inputs": {"who": "the team"}, "since": 7}));
        let hears = vec!["ticket  hook  begins when called".to_string()];
        let page = listening_page(&listens, &hears, true);
        assert_eq!(page.len(), 4, "{page:?}");
        page.iter().for_each(|line| sentence(line));
        assert!(page[0].contains("listening since 7"), "{}", page[0]);
        assert!(page[1].contains("who=the team"), "{}", page[1]);
        assert_eq!(page[2], "  ticket  hook  begins when called");
        assert!(
            page[3].contains(&format!("bisa workflow listeners --goal {}", listens.id)),
            "{}",
            page[3]
        );

        // With no node to hear them, its events are heard once one runs.
        let unheard = listening_page(&listens, &hears, false);
        assert_eq!(unheard.len(), 5, "{unheard:?}");
        assert!(unheard[4].contains("bisa node"), "{}", unheard[4]);

        // Once captured, the next step is to look at what it hears.
        let next = next_step_line(&listens, true);
        sentence(&next);
        assert!(
            next.contains(&format!("bisa status {}", listens.id)),
            "{next}"
        );
    }

    #[test]
    fn a_paused_goal_s_page_says_why_and_how_it_listens_again() {
        let paused = goal(json!({
            "since": 7,
            "paused": {
                "reason": {"reason": "run_failed", "run": "01BX5ZZKBKACTAV9WEVGEMMVRZ"},
                "at": 9,
            },
        }));
        let page = listening_page(&paused, &[], false);
        assert_eq!(page.len(), 2, "{page:?}");
        page.iter().for_each(|line| sentence(line));
        assert!(
            page[0].contains("paused since 9") && page[0].contains("01BX5ZZKBKACTAV9WEVGEMMVRZ"),
            "{}",
            page[0]
        );
        assert!(
            page[1].contains(&format!("bisa run {}", paused.id)),
            "{}",
            page[1]
        );
    }

    #[test]
    fn a_step_s_line_says_the_branches_it_chose_and_what_diverted_it() {
        let record = |state: StepState| StepRecord {
            state,
            ..StepRecord::default()
        };
        let branch = |name: &str| Branch::new(name).expect("a branch");
        let plain = record(StepState::Done { branches: vec![] });
        assert_eq!(step_detail(&plain), "");
        let chose = record(StepState::Done {
            branches: vec![branch("mail"), branch("chat")],
        });
        assert_eq!(step_detail(&chose), "  → mail · chat");
        let diverted = record(StepState::Diverted { by: branch("late") });
        assert_eq!(step_detail(&diverted), "  ↪ diverted by late");
        let failed = StepRecord {
            state: StepState::Failed,
            error: Some("the check did not pass".into()),
            ..StepRecord::default()
        };
        assert_eq!(step_detail(&failed), "  — the check did not pass");
    }

    #[test]
    fn a_run_s_page_says_who_started_it() {
        assert_eq!(started_by_line(&run::StartedBy::You), "started:   by you");
        let by_signal = run::StartedBy::Event {
            event: bisa_core::SignalSource::Signal,
            detail: Some("report.ready".into()),
        };
        assert_eq!(
            started_by_line(&by_signal),
            "started:   by signal report.ready"
        );
    }
}
