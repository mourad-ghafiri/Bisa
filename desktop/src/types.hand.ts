/**
 * Hand-written wire types — the gap the generator cannot reach.
 *
 * `types.gen.ts` is produced by `just gen-types` from every Rust type that
 * derives `schemars::JsonSchema`: the node's DTOs and all of `bisa-core`.
 * The types below come from `bisa-store`, `bisa-engine` and
 * `bisa-harness`, which do not derive it, so their shapes are mirrored
 * here by hand and must be kept in step with:
 *
 *   store   → src/{agents,skills,mcp,teams,conversation,recall,members,governance}.rs
 *   engine  → src/{events,gates,ops}.rs
 *   harness → src/{event,types}.rs
 *
 * The one exception is `bisa-harness`'s model-plan trio (`ModelPlan`,
 * `ModelChoice`, `ModelStrategy`): the agent DTOs carry them, so they derive
 * `JsonSchema` and are generated. `AgentDef` below imports `ModelPlan` from
 * `types.gen` rather than mirroring it. `Effort` and `EffortChoice` are the
 * core's (`bisa_core::effort`) and generated too, so the fields that carry
 * one here name the generated type and this file declares neither.
 *
 * Adding `schemars::JsonSchema` to those crates would move each of these into
 * the generated bundle unchanged — that is the intended end state; this file
 * exists because this wave owned only the node crate.
 */

import type {
  AgentOrigin,
  Text,
  AskKind,
  Audience,
  ChannelKind,
  Assignee,
  Gate,
  Origin,
  RespondPolicy,
  RosterPolicy,
  BudgetSpent,
  ChangedFile,
  ConversationKind,
  GitFileRow,
  GitInProgress,
  GitStatusInfo,
  ReviewNote,
  Goal,
  GoalStatus,
  GuidanceInfo,
  Effort,
  ModelPlan,
  Project,
  RunSummary,
  StepId,
  RunId,
  WorkflowId,
  WorkflowRun,
  WorkItemSpec,
  Workstream,
  WorkstreamKind,
  WorkstreamState,
  ContextRef,
} from "./types.gen";

// ---------------------------------------------------------------------------
// bisa-store
// ---------------------------------------------------------------------------

/**
 * Where a definition came from. Mirrors `bisa_core::origin`.
 *
 * There is no default on the wire, deliberately: provenance that can be
 * omitted is provenance that can be laundered, and a default would silently
 * turn the platform's own agent into an ordinary removable one.
 */
/** An agent's origin. `core` is a case the others cannot hold. */
/**
 * The General Agent — the platform's own staffer. Always present, always
 * enabled, never removable; only its harness and model plan may be edited.
 * Authority for these ids is `bisa_core::AgentId`.
 */
const CORE_AGENT_ID = "general-agent";

/** The Workflow Agent — the platform's own workflow designer. Same rules. */
export const WORKFLOW_AGENT_ID = "workflow-agent";

/**
 * The two core agents that hold a record — a key, a prompt, a conversation —
 * in the order the core spells them. The third core agent, the
 * Decision-Making Agent, holds none, so it is no id of this list.
 */
export const CORE_AGENT_IDS: readonly string[] = [CORE_AGENT_ID, WORKFLOW_AGENT_ID];

/** Is this one of the platform's own agents? */
export function isCoreAgent(a: { origin: AgentOrigin }): boolean {
  return a.origin === "core";
}

/** The catalog slug a definition was installed from, if any. */
export function catalogSlug(origin: Origin | AgentOrigin): string | null {
  return typeof origin === "object" ? origin.catalog.slug : null;
}

/**
 * A skill in the workspace library.
 *
 * Skills are one library, not a copy per agent: an agent carries skill **ids**
 * in `AgentDef.skills`. `description` is the line a model reads to decide
 * whether to open the skill at all, so it is required rather than derived.
 */
export interface SkillDef {
  id: string;
  name: string;
  description: string;
  tags?: string[];
  markdown: string;
  /**
   * Where this came from. Recorded at creation and preserved across updates
   * like `created_at` — provenance is recorded, never accepted from a caller.
   */
  origin: Origin;
  created_at: number;
}

/**
 * An installed pet: an animated companion, in Codex's package format.
 *
 * The field names are `camelCase` because that is what the format says, and
 * speaking somebody else's format exactly is the point — a pet made for Codex
 * works here untouched, and one installed here can be copied back out. Mirrors
 * `bisa_store::pets::PetDef`.
 */
export interface PetDef {
  /** Also the directory name, so it is validated as a slug. */
  id: string;
  displayName: string;
  description: string;
  /** Relative to the package directory; resolved inside it, never outside. */
  spritesheetPath: string;
  /** The pack's frame rate, when it names one; the durations decide. */
  fps?: number;
  defaultAnimation?: string;
  /** One row per state the pack animates: its frames and each frame's duration. */
  animations?: Record<string, PetAnimation>;
  /** Who the pet is, in the pack's own words (`x-bisa-pets`). */
  "x-bisa-pets"?: PetFlavour;
  /** `catalog` for one the platform ships; absent for one installed here. */
  origin?: "catalog" | "local";
}

export interface PetAnimation {
  row: number;
  frames: number;
  frameDurationsMs?: number[];
}

export interface PetFlavour {
  pack?: string;
  slug?: string;
  tagline?: string;
  archetype?: string;
  personality?: string;
  mood?: string;
}

import type { ArtifactRef, AttachmentRef } from "./types.gen";
import { t } from "./i18n/l10n.mjs";

export interface AgentDef {
  id: string;
  name: string;
  /** The agent's picture — an attachment this machine holds (ide/14 §Photos). */
  photo?: AttachmentRef | null;
  description?: string | null;
  system_prompt: string;
  /** Harness id this agent runs on. */
  harness: string;
  /**
   * Models this agent may run on, best-first, plus the strategy for choosing
   * among them. An empty list = the harness's own default model.
   */
  models: ModelPlan;
  /** Skill library ids, resolved at launch. */
  skills: string[];
  /** MCP registry ids, resolved at launch. */
  mcps: string[];
  /** Categories for grouping and filtering. */
  tags?: string[];
  respond: RespondPolicy;
  /** The agent's own pubkey; the secret lives in the keystore. */
  pubkey: string;
  /**
   * Where this definition came from. `core` is the platform's own agent: it
   * cannot be removed or disabled, and only `harness`, `models` and
   * `decision_making` may be edited.
   */
  origin: AgentOrigin;
  enabled: boolean;
  /** Let the Decision-Making Agent decide for this agent — one of the three things a core agent may change. */
  decision_making?: boolean;
  created_at: number;
}

/**
 * A team is agents and humans working one goal together.
 *
 * Members are `Assignee`s — the same type goals and projects use. A team
 * inside a team is refused at validation: one level needs no cycle detection.
 */
export interface TeamDef {
  id: string;
  name: string;
  purpose?: string | null;
  /** The team's picture — an attachment this machine holds (ide/14 §Photos). */
  photo?: AttachmentRef | null;
  members: Assignee[];
  /** A disabled team takes no work; membership events say so in its channels. */
  enabled?: boolean;
  tags?: string[];
  /**
   * Where this came from. Recorded at creation and preserved across updates
   * like `created_at` — provenance is recorded, never accepted from a caller.
   */
  origin: Origin;
  created_at: number;
}

export interface ChannelDef {
  id: string;
  name: string;
  topic?: string | null;
  kind: ChannelKind;
  /** Who may read: the whole workspace, or exactly these principals. */
  audience: Audience;
  /**
   * The standing roster: agent definition ids that belong here.
   *
   * **A roster is a directory, not a subscription.** It is not `audience`,
   * which restricts who can read. A rostered agent does not auto-reply —
   * agents speak when addressed. What the roster buys is the header, the
   * ordering of the @-picker, and the channel handle: mentioning the channel's
   * own id expands to these agents **plus the core agent**, which belongs to
   * every room by construction and is therefore part of what "everybody here"
   * means.
   */
  roster: RosterPolicy;
  tags?: string[];
  /**
   * Where this came from. Recorded at creation and preserved across updates
   * like `created_at` — provenance is recorded, never accepted from a caller.
   */
  origin: Origin;
  created_at: number;
}

/**
 * The principals a channel is encrypted to. A workspace-wide channel has none
 * listed — every member reads it — so callers that draw "who is here" get an
 * empty list and fall back to the roster.
 */
export function audiencePrincipals(channel: { audience: Audience } | null | undefined): string[] {
  const a = channel?.audience;
  return a && a.audience === "restricted" ? a.principals : [];
}

/** The agent ids a channel's `@handle` expands to. `everyone` is the general channel's. */
export function rosterAgents(channel: { roster: RosterPolicy } | null | undefined): string[] {
  const r = channel?.roster;
  return r && r.policy === "listed" ? (r.agents ?? []) : [];
}

/** The people (pubkeys of hosted members) on a channel's roster — what a guest reaches (14-collaboration). */
export function rosterHumans(channel: { roster: RosterPolicy } | null | undefined): string[] {
  const r = channel?.roster;
  return r && r.policy === "listed" ? (r.humans ?? []) : [];
}

export interface MessageRow {
  id: string;
  scope_id: string;
  author: string;
  content: string;
  /** An agent's reasoning before its words, when its harness said it (13 — Conversations); never on a person's post. */
  thinking?: string | null;
  /** The platform's own sentence behind a post it authored — the message of the catalog, rendered in the reader's language (17 — Internationalisation); `content` is its English. */
  said?: import("./types.gen").Text | null;
  /** The chips this message carried (ide/09), in the order they were attached. */
  context?: ContextRef[];
  created_at: number;
  reply_to?: string | null;
  retracted: boolean;
  /** Files this message carries, in the order they were composed. */
  attachments?: MessageAttachment[];
  /** What the author made for the reader to look at (ide/12), in order. */
  artifacts?: MessageArtifact[];
}

/** An artifact as a reader sees it: the descriptor, and whether its bytes are here. */
export interface MessageArtifact extends ArtifactRef {
  present: boolean;
}

/** One artifact of a conversation with the message it rode on — `GET /artifacts/{scope}`. */
export interface ArtifactListRow extends ArtifactRef {
  message_id: string;
  scope_id: string;
  author: string;
  created_at: number;
  present: boolean;
}

/** An attachment as a reader sees it: what it is, and whether it is here. */
export interface MessageAttachment extends AttachmentRef {
  /**
   * Whether the bytes are on **this** machine.
   *
   * `false` is ordinary, not an error: a file does not travel with its
   * message — the descriptor syncs and the bytes are fetched from whoever has
   * them — so a peer's attachment arrives named and absent.
   */
  present: boolean;
}

export interface ReactionRow {
  id: string;
  target_id: string;
  author: string;
  emoji: string;
  retracted: boolean;
}

export interface RecallRecord {
  slug: string;
  value: string;
  /** sha256 hex of the value bytes — the base hash for guarded updates. */
  hash: string;
  updated_at: number;
  /** `[[slug]]` wiki-links found in the value. */
  links: string[];
  /** No other record links here (review candidates; never auto-deleted). */
  orphan: boolean;
}

/** A member as `GET /workspace` lists them — the generated shape, named as the lists read it. */
export type MemberRow = import("./types.gen").WorkspaceMember;

type GatePolicy = import("./types.gen").GatePolicy;

/// Who may decide each gate, as `GET /governance` answers. A policy is the
/// generated `GatePolicy`: `admins` is the owner and every admin, `members`
/// adds every member, `listed` names pubkeys and `team:<id>` refs; a guest
/// never decides (14-collaboration).
export interface Governance {
  /// Adopting a proposed workflow, an `approval` step of a run, an amendment.
  approval: GatePolicy;
  /// Answering a `human` step or an agent's question.
  escalation: GatePolicy;
  /// Pushing a branch or opening a PR leaves the machine, so it passes a gate.
  publish: GatePolicy;
}

// ---------------------------------------------------------------------------
// bisa-engine
// ---------------------------------------------------------------------------

interface GateResolution {
  approve: boolean;
  answer?: string | null;
  decision_event_id: string;
}

export interface GateEntry {
  id: string;
  /**
   * Where the question is asked: the goal it is about, or the run of the
   * workspace — whose journal holds the decision and whose Inbox row (the
   * run's workflow's) shows it.
   */
  home: import("./types.gen").Home;
  work_item?: string | null;
  /** The run and step this gate belongs to, when it is a step's. */
  run?: RunId | null;
  step?: StepId | null;
  gate: Gate;
  /**
   * What is being decided: `approval:<run>/<step>`, `step:<run>/<step>`,
   * `adopt:<workflow>@<revision>`, `amend:<run>@<workflow>`, a permission
   * summary.
   */
  subject: string;
  /** Human-facing question for the inbox. */
  question: string;
  expects: AskKind;
  opened_at: number;
  resolution?: GateResolution | null;
}

/**
 * One row of the engine's model-health ledger (`GET /models/health`).
 *
 * Live runtime state, not truth: it is per-process and a restart forgets it,
 * so an empty list means "nothing to report", never "no models".
 */
export interface ModelHealthRow {
  harness: string;
  model: string;
  /** Unix seconds, present only while the cooldown is still in the future. */
  cooldown_until?: number;
  /** Seconds from now until the cooldown expires — what a badge renders. */
  retry_in_secs?: number;
  consecutive_failures: number;
  in_flight: number;
}

/** Where a home stands after a decision: a goal's status, or a run of the workspace's. */
export type HomeStatus =
  | { of: "goal"; status: GoalStatus }
  | { of: "run"; status: import("./types.gen").RunStatus };

/** What a decision settled, through the home it was filed at (`POST /goals/{id}/decide`, `POST /runs/{rid}/decide`). */
export interface DecideOutcome {
  home: import("./types.gen").Home;
  gate: Gate;
  approve: boolean;
  status: HomeStatus;
  /** An adoption that armed a public hook start: its secret, shown once. */
  secrets?: import("./types.gen").HookSecret[];
}

/** The two phases the Workflow Agent works in; anything else is the goal's status. */
type GuidedPhase = import("./types.gen").GuidancePhase;

/**
 * The engine's SSE bus payloads, mirrored by hand.
 *
 * `EnginePayload` does not derive `JsonSchema` — it rides `GET /events`
 * rather than a route, so nothing generates it and this copy is the only
 * TypeScript that knows it exists. **That makes it the file's worst drift
 * risk: a missing variant is not a type error, it is a `switch` arm that
 * never fires.** M11 found six absent at once (`agent_thinking`,
 * `agent_replied`, the three `workstream_*`, `model_switched`) — long enough
 * for a file tree to be written that could not refresh on a commit, and for
 * the author to conclude from the missing type that the event did not exist.
 *
 * Check it against `EnginePayload` in `crates/bisa-engine/src/events.rs`.
 * The wire spelling is `snake_case` with `tag = "type"`, from the serde
 * attribute on the enum — so `WorkstreamCommitted` is `workstream_committed`, and
 * grepping the generated types for it will always come back empty.
 */
export type EnginePayload =
  | { type: "session"; event: SessionEvent }
  | { type: "scheduled"; harness: string }
  | { type: "execution_ended"; outcome: import("./types.gen").ExecutionOutcome }
  /** One session's presence changed — folded in the engine, never per token. */
  | { type: "session_state"; live_run: string; presence: import("./types.gen").SessionRow }
  | { type: "session_gone"; live_run: string }
  | { type: "gate_opened"; gate_id: string; gate: Gate; question: string }
  | { type: "question_asked"; gate_id: string; text: string; expects: AskKind }
  /** The Workflow Agent's standing on a guided goal moved; mirrors the `guidance` journal fact. */
  | {
      type: "guided";
      phase: GuidedPhase;
      status: import("./types.gen").GuidanceStatus;
      detail?: string | null;
      session?: string | null;
    }
  | { type: "gate_decided"; gate_id: string; gate: Gate; approve: boolean }
  | { type: "result_accepted" }
  | { type: "run_started"; run: RunId; workflow: WorkflowId }
  /** A run made behind the goal's live run; `position` is its place in the queue, 1 next. */
  | { type: "run_queued"; run: RunId; workflow: WorkflowId; position: number }
  | { type: "run_finished"; run: RunId; workflow: WorkflowId; outcome: "done" | "failed" }
  /** A run cancelled, live or queued: stopped or restarted by a person, withdrawn, closed with its goal, or retired with its workflow. */
  | { type: "run_cancelled"; run: RunId; workflow: WorkflowId; cause: import("./types.gen").CancelCause }
  /** `state` is the `StepState` tag, `kind` the step kind's tag. */
  | { type: "step_changed"; run: RunId; workflow: WorkflowId; step: StepId; state: string; kind: string }
  | { type: "goal_closed"; reason: import("./types.gen").ClosureReason }
  /** A goal came into existence — the workspace's shape changing; the activity feed's *Workspace* concept. */
  | { type: "goal_created"; goal: string; origin: import("./types.gen").GoalOrigin }
  | { type: "workflow_proposed"; workflow: WorkflowId; revision: number; gate_id?: string | null }
  /** `designed`: the Workflow Agent's hand — a proposal recorded, an amendment applied alone — and the Inbox's notice on the workflow's row; a person's own save is `false`. */
  | { type: "workflow_changed"; workflow: WorkflowId; revision: number; designed: boolean }
  /** A workflow gone from the library and the pickers; the runs that copied it keep their copy. The designer open on it leaves. */
  | { type: "workflow_deleted"; workflow: WorkflowId }
  /** A workflow put away or taken back out — a mark, never a status. */
  | { type: "workflow_archived"; workflow: WorkflowId; archived: boolean }
  | { type: "attachment_changed"; project: string; attached: boolean }
  /** A goal put away or taken back out — a mark, never a status. */
  | { type: "goal_archived"; goal: string; archived: boolean }
  | { type: "goal_deleted"; goal: string }
  /** A project put away or taken back out; every session in it was stopped. */
  | { type: "project_archived"; project: string; archived: boolean }
  | { type: "project_deleted"; project: string }
  /** A person gave the goal a document — a file under its `documents/` folder as context. */
  | { type: "document_added"; goal: string; file: AttachmentRef }
  | { type: "settings_changed"; scope: string; project?: string | null; keys: string[] }
  | {
      type: "file_changed";
      scope: string;
      id: string;
      path: string;
      kind: "created" | "modified" | "removed" | "renamed" | "rescan";
      from?: string | null;
      ignored: boolean;
      /** The path is a folder: a whole tree made, moved or copied in one frame. `false` for a path that is gone. */
      dir?: boolean;
    }
  | {
      /** A language server spoke (ide/10): diagnostics, or its own lifecycle. `uri` fields are root-relative. */
      type: "lsp";
      scope: string;
      id: string;
      language: string;
      method: string;
      params: unknown;
    }
  | { type: "project_created"; project: string; slug: string; origin: import("./types.gen").ProjectOrigin }
  /** Nobody is set to commit in a project and nothing resolved it — a person is asked. */
  | {
      type: "committer_needed";
      project: string;
      slug: string;
      /** The primary — where the answer is written. */
      workstream: string;
      reason: import("./types.gen").CommitterReason;
      origin: import("./types.gen").ProjectOrigin;
      /** The person's global git pair, offered for pinning. */
      global?: import("./types.gen").GitIdent | null;
    }
  /** Who commits in a project was set — its repository's local identity. */
  | { type: "committer_set"; project: string; workstream: string; identity: import("./types.gen").GitIdent }
  /** The person's git setup changed through the platform: a profile, an SSH key, a code host account. Never a value. */
  | { type: "git_setup_changed"; what: "profiles" | "keys" | "accounts" | "identity" }
  /** A connector definition or one of this machine's connector accounts changed. Never a value. */
  | { type: "connectors_changed"; what: "definitions" | "accounts" }
  /** An addon was installed, removed, enabled, disabled or re-granted (18 — Addons). Never a value. `crates/bisa-engine/src/addons.rs`. */
  | { type: "addons_changed"; what: "installed" | "removed" | "enabled" | "disabled" | "grants"; id: string }
  /** A conversation was started; its origin says what it is about (13 — Conversations). */
  | { type: "conversation_created"; id: string; origin: import("./types.gen").ConversationOrigin }
  /** A conversation's record moved: a list re-reads on it. `"mode"` is the conversation's tool ceiling changing (09-agents-in-the-ide). */
  | { type: "conversation_changed"; id: string; change: "renamed" | "archived" | "unarchived" | "deleted" | "mode" }
  /** The change ledger moved for a conversation about a checkout: re-read `GET /conversations/{id}/changes`. */
  | { type: "changes_moved"; conversation: string; workstream: string; pending: number }
  /** A settle or a restore landed — an activity fact (`changes.settled`). */
  | {
      type: "changes_settled";
      conversation: string;
      act: "keep" | "undo" | "restore";
      files: number;
      skipped: { path: string; why: string }[];
    }
  /** The Tool & Commands Guard put a call to a person in the conversation, in words. */
  | { type: "ask_opened"; conversation: string; ask: import("./types.gen").ConversationAsk }
  /** An open ask was answered. */
  | { type: "ask_settled"; conversation: string; ask_id: string; allowed: boolean }
  | { type: "project_changed"; project: string }
  | { type: "workstream_edited"; workstream: string; project: string }
  /** A note was created, changed or deleted; `hash` (the body's) lets an open editor tell its own save from somebody else's, `by` names the agent whose tool wrote. */
  | { type: "note_changed"; note: string; scope: string; scope_id?: string; hash: string; by?: string }
  /** A drawing was created, changed or deleted (19 — Drawings); `hash` lets an open canvas tell its own save from somebody else's. */
  | { type: "drawing_changed"; drawing: string; scope: string; scope_id?: string; hash: string }
  /** An agent asked the canvas to draw (19): the desktop performs it in a canvas and answers. */
  | { type: "drawing_request"; id: string; request: import("./types.gen").DrawRequest; scope: import("./types.gen").DrawScope }
  | { type: "agent_thinking"; scope: string; agent: string }
  /**
   * The words and the thinking an agent's turn added since the last frame — either may be empty — and
   * the tool it runs now, as it stands: a frame replaces it, and `null` says the tool ended (13 — Conversations).
   */
  | { type: "agent_streamed"; scope: string; agent: string; text: string; thinking: string; working: string | null }
  /** The turn is over; `message` is the id the reply landed as, so the live row waits for it — null when nothing was posted. */
  | { type: "agent_replied"; scope: string; agent: string; posted: boolean; message: string | null }
  | {
      type: "workstream_opened";
      workstream: string;
      project: string;
      path: string;
      branch: string | null;
      /** Where the branch came from, in a line the node wrote: *a new branch*, *the branch origin/x*, *pull request #12*, *a copy*. */
      source: string;
    }
  | { type: "workstream_changed"; workstream: string; state: WorkstreamState }
  | { type: "workstream_committed"; workstream: string; branch: string; commit: string }
  /** The Tool & Commands Guard judged a tool call; `subject` is the redacted command or input. */
  | {
      type: "guard_decided";
      session?: string | null;
      tool: string;
      subject: string;
      verdict: import("./types.gen").GuardVerdict;
      by: import("./types.gen").GuardJudge;
      rule?: string | null;
      reason?: string | null;
    }
  /** The Decision-Making Agent answered (or did not) for one of the eleven decision points. */
  | {
      type: "judged";
      judgement: import("./types.gen").Judgement;
      agent?: string | null;
      run?: RunId | null;
      step?: StepId | null;
    }
  /** Secrets became placeholders before text left for an agent — how many, of which kinds, at which seam. */
  | { type: "redacted"; count: number; kinds: string[]; at: string }
  /** A folder of a checkout started or stopped being served by the node (ide/18). */
  | { type: "server_changed"; workstream?: string }
  /** An agent asked the embedded browser to do something (ide/18): the desktop performs it and answers. */
  | { type: "browser_request"; id: string; request: import("./types.gen").BrowserRequest; scope: import("./types.gen").BrowserScope; headless: boolean }
  /** This machine's mobile toolchain was examined again, or a device was booted, shut down or made (ide/19). */
  | { type: "mobile_development_changed"; what: import("./types.gen").MobileDevelopmentChange }
  /** An installed MCP server was probed (06 § MCP servers): Settings and the agent editor read its health again. */
  | { type: "mcp_probed"; id: string; ok: boolean }
  /** An account of a connector installed here was checked (03 § Connectors): Settings › Connectors reads its health again. */
  | { type: "connector_checked"; connector: string; account: string; ok: boolean }
  /**
   * What a person approved at the Publish gate did not go out — a push, a pull
   * request, a merge (ide/08): `what` is the act as the gate asked it, `reason`
   * the refusal in words.
   */
  | { type: "workstream_publish_failed"; workstream: string; project: string; what: string; reason: string }
  /** A workstream script ran, or was skipped for want of approval here (ide/07 §Workstream scripts). */
  | {
      type: "workstream_script_ran";
      workstream: string;
      project: string;
      phase: import("./types.gen").ScriptPhase;
      ok: boolean;
      /** The outcome in words, then the tail of what the script printed. */
      output: string;
    }
  | {
      type: "model_switched";
      work_item?: string;
      from: string;
      to: string;
      reason: string;
      retry_in_secs: number;
      after_progress: boolean;
    }
  /**
   * An event was durably queued for the start event that heard it — or, heard by none (`listener`
   * null), kept for the waits that may. `listener` is a `ListenerKey` in its wire form:
   * `workspace:<workflow>/<step>` or `goal:<goal>/<step>`; `source` is what the event came from.
   */
  | { type: "signal_received"; signal: string; listener: string | null; source: string }
  /** What a start event's occurrence did: a run (`goal` names the goal it is for; absent for a run of the workspace), or none, refused by the start's guard. */
  | {
      type: "listener_fired";
      listener: string;
      signal: string;
      outcome: { outcome: "started"; run: RunId; goal?: string | null } | { outcome: "skipped"; reason: string };
    }
  /** A start event could not start its run, or could not be armed; `signal` names the occurrence when there was one. */
  | { type: "listener_failed"; listener: string; signal?: string | null; error: string }
  /** A host began or stopped listening — `workspace:<workflow>` a library workflow turned On or Off, `goal:<goal>` a goal armed or cleared. */
  | { type: "listening_changed"; host: string; on: boolean }
  /** An event on a live step fired its boundary: a divert stopped the step and took the boundary's flows; otherwise it acted beside the step. */
  | { type: "boundary_fired"; run: RunId; workflow: WorkflowId; step: StepId; boundary: string; diverts: boolean }
  | { type: "paused" }
  | { type: "resumed" }
  /** A person on another node joined, left or changed role (14-collaboration). */
  | { type: "people_changed"; pubkey: string; change: import("./types.gen").PeopleChange; label?: string | null }
  /** An invitation moved — made, claimed, admitted, refused, withdrawn, expired. Never a secret. */
  | { type: "invite_changed"; invite: import("./types.gen").Invite }
  /** A person's message is held from agents, and why; the owner may release it. */
  | { type: "message_held"; scope: string; event: string; author: string; reason: import("./types.gen").HeldReason }
  | { type: "message_released"; scope: string; event: string }
  /** The content screen read what an agent was about to read from outside — the source and the verdict, never the words (11 — Security). */
  | { type: "content_screened"; scope?: string | null; agent: string; source: string; verdict: import("./types.gen").ContentVerdict }
  /** What this node holds of a workspace it is a guest of moved. */
  | { type: "hosted_changed"; host: string; scope?: string | null }
  /** The relays this node talks to changed. */
  | { type: "relays_changed" };

export interface EngineEvent {
  goal?: string | null;
  /** The workflow of the run the payload is about, when a run is behind it — what a run of the workspace, which no goal holds, is filed under. */
  workflow?: string | null;
  /** The run behind the payload, when there is one — a goal's or the workspace's. */
  run?: string | null;
  work_item?: string | null;
  payload: EnginePayload;
}

// ---------------------------------------------------------------------------
// bisa-harness
// ---------------------------------------------------------------------------

export type Outcome =
  | { outcome: "completed" }
  | { outcome: "aborted" }
  | { outcome: "failed"; error: string }
  /** The model went away mid-turn — a retry on the next model, not a failure. */
  | { outcome: "model_unavailable"; model: string; reason: string; retry_after?: number | null }
  | { outcome: "suspended"; reason: string };

/** Something a harness asked for and stopped on; `InputKind` is generated. */
type InputRequest = import("./types.gen").InputRequestKind & { id: string; parent?: string | null };

type LifecycleEvent =
  | { type: "started" }
  | { type: "process_started"; pid?: number | null }
  | { type: "ended"; outcome: Outcome; is_terminal: boolean }
  | { type: "parked" }
  | { type: "revived" }
  | { type: "input_requested"; request: InputRequest }
  | { type: "input_resolved"; id: string };

type ProgressEvent =
  | { type: "turn_started" }
  | { type: "turn_ended" }
  | { type: "subagent_started"; id: string; name: string; description: string }
  | { type: "subagent_ended"; id: string; ok: boolean }
  /** A progress event raised inside a sub-agent — never the agent's own. */
  | { type: "nested"; parent: string; event: ProgressEvent }
  /** `id` is the harness's own id for the call, when it has one — what ties the end to the start. */
  | { type: "tool_started"; name: string; args_summary: string; tier: "read" | "write" | "exec"; id?: string | null }
  | { type: "tool_ended"; name: string; ok: boolean; id?: string | null }
  | { type: "text_delta"; text: string }
  /** The model's reasoning as it streams — never the reply. */
  | { type: "thinking_delta"; text: string }
  | { type: "cost_delta"; input_tokens: number; output_tokens: number; usd_cents: number };

export type SessionEvent =
  | { tier: "lifecycle"; event: LifecycleEvent }
  | { tier: "progress"; event: ProgressEvent }
  | { tier: "raw"; event: unknown };

// ---------------------------------------------------------------------------
// Response envelopes assembled with `json!` in the node (no DTO struct yet)
// ---------------------------------------------------------------------------

/// Channels and DMs are listed with their unread count and their latest live
/// post — who, the first words, when; `null` when nothing was said yet —
/// wrapped around the definition (`GET /channels`, `GET /dms`).
export interface ChannelListEntry {
  channel: ChannelDef;
  unread_count: number;
  latest: import("./types.gen").Representative | null;
}

/**
 * `GET /goals/{id}` — the goal with everything a screen needs beside it.
 *
 * `status` is the projection the node already computed; `run` is the current
 * run whole (the frozen workflow and every step's record) — the live run,
 * else the latest that started, never a queued one — and `runs` every run
 * the goal has had, newest first: a queued one carries its `position`, a
 * cancelled one its `cause`.
 */
export interface GoalView {
  goal: Goal;
  status: GoalStatus;
  run: WorkflowRun | null;
  runs: RunSummary[];
  work_items: WorkItemSpec[];
  spent: BudgetSpent;
  active: number;
  pending_gates: GateEntry[];
  guidance: GuidanceInfo;
  /** The same run-as-a-strip the list rows carry, so a header needs no second shape. */
  strip: import("./types.gen").RunStrip;
  /** Who the goal waits on — the core's one rule, computed by the node. */
  holder: import("./types.gen").Holder;
  last_activity_at: number;
  /** The goal's own designs — workflows born of it, out of the library. */
  designs: import("./types.gen").WorkflowRow[];
  /** The goal's standing while its workflow begins on events: armed, or paused after a failed run or a spent budget; `null` for a goal whose work begins by hand. */
  listening?: import("./types.gen").Listening | null;
}

/**
 * `payload` is the generated union, not `Record<string, unknown>`.
 *
 * It used to be the loose shape, which is why every reader of a journal event
 * cast field by field and why a payload variant could be dropped from the activity
 * with nothing to say so. `JournalPayload` reaches `types.gen.ts` because the
 * Pulse ships it (`dto::PulseEvent`), so the discrimination is free.
 */
export interface JournalEntry {
  /** The journal it was written to: a goal's, or a run of the workspace's. */
  home: import("./types.gen").Home;
  author: string;
  at: number;
  payload: import("./types.gen").JournalPayload;
}

/** `GET /work-items/{item}`: the item, where it is filed and what it yielded. */
export interface WorkItemDetail {
  home: import("./types.gen").Home;
  /** What the home reads as: the goal's title (else its statement), or the run's workflow and number. */
  label: string | null;
  item: WorkItemSpec;
  result: unknown | null;
  has_result: boolean;
}

/** How to run a harness interactively — the bare command, not the protocol
 * invocation the engine drives. `null` for anything with no such form. */
interface HarnessLaunch {
  program: string;
  args?: string[];
  /**
   * What continues this harness's latest session in the directory it is started
   * in — `--continue`, `--resume`, whatever the tool calls it. Absent or empty
   * for a harness with no such form, which is most of them: a flag invented
   * here would be a command that fails in somebody's terminal.
   */
  resume_args?: string[];
}

export interface HarnessRow {
  id: string;
  label: string;
  tier: "builtin" | "preset" | "custom";
  installed: boolean;
  /** What the binary reports. Never a filesystem path — that is `path`. */
  version: string | null;
  /** Where a bare PATH lookup found it, when that is how it was probed. */
  path: string | null;
  launch: HarnessLaunch | null;
  detail: string | null;
  install_hint: string | null;
  /** How it is installed, in the official page's words — for the compiled-in harnesses the platform knows the page of (16 — The setup gate). */
  install: import("./types.gen").InstallHint | null;
  /** The Tool & Commands Guard can veto this harness's tool calls before they run. */
  tool_guard: boolean;
  /** The guard can hand it a rewritten input — a redacted placeholder restored before the tool runs. */
  input_rewrite: boolean;
}

/**
 * One work item, with the home it is filed at — its goal, or the run of the
 * workspace that made it — and the words that home reads as.
 *
 * Every work-item route is addressed by the item's id alone
 * (`/work-items/{item}`). `label` rides along because an item's
 * instructions do not say which piece of work they belong to, and fetching
 * the home per row would put the N+1 back that the workspace-wide route
 * exists to remove.
 */
export interface WorkItemRef {
  home: import("./types.gen").Home;
  label: string | null;
  item: WorkItemSpec;
}

/** One workstream, with the name of the project it checks out. */
export interface WorkstreamRef {
  project_name: string | null;
  /** Where the checkout is — resolved from the kind, never stored on the record. */
  path: string | null;
  /** Whether the checkout is on disk. A record can outlive its tree. */
  exists: boolean;
  workstream: Workstream;
}

/** Where one of a project's workstreams checks out (`GET /projects/{pid}/workstreams`). */
export interface WorkstreamCheckout {
  workstream: string;
  path: string;
  exists: boolean;
}

/** One model a harness lists (`GET /harnesses/{id}/models`), mirroring `bisa_harness::ModelInfo`. */
export interface ModelInfo {
  id: string;
  label?: string | null;
  /**
   * The effort levels this model takes on this harness, lowest first. Always
   * present; empty means the harness lists none for it.
   */
  efforts: Effort[];
}

export interface WorkspaceInfo {
  pubkey: string;
  npub: string;
  data_dir: string;
  /** Where this machine's diagnostic log is — the store's word, `logs/` under `data_dir`. */
  logs_dir: string;
  /** The owner first, then the people hosted here. Relays are the `sync.relays` setting; the wire is `GET /sync`. */
  members: MemberRow[];
  /** What the last open and index rebuild found wrong and worked around; empty for a sound workspace. An older node says nothing. */
  problems?: WorkspaceProblem[];
}

/** One thing the open or the rebuild found wrong and worked around — a file moved under `quarantine/`, a record skipped, a run ended at the open (`bisa_store::WorkspaceProblem`). */
export interface WorkspaceProblem {
  kind: "unreadable" | "quarantined" | "recreated" | "index_disagrees" | "rebuild_skipped" | "settings_layer_unreadable" | "orphan_run" | "stale_row" | "duplicate_dispatch";
  /** The file or record it is about — a path, or an id when no file names it. */
  path: string;
  /** The sentence a person reads. */
  text: Text;
  /** Where the file was moved, when it was. */
  quarantined?: string;
  /** Unix seconds, when it was found. */
  at: number;
}

export interface SearchHit {
  id: string;
  title: string | null;
  statement: string;
  status: GoalStatus;
}

/** SSE envelope: one connection, three streams — and the bus's own `system` frame, for the client, never a subscriber. */
export type BusFrame =
  | { stream: "engine"; payload: EngineEvent }
  | { stream: "conversation"; payload: ConversationFrame }
  | { stream: "inbox"; payload: InboxFrame }
  | { stream: "system"; payload: SystemFrame };

/** What the node tells a client's bus about itself: it lagged, and `dropped` events are gone. */
export interface SystemFrame {
  kind: "lagged";
  dropped: number;
}

export interface ConversationFrame {
  scope: string;
  kind: number;
  event_id?: string;
  author?: string;
  snippet?: string;
  unread_count?: number;
  /**
   * What now stands as the room's latest live post, by the node's own rule —
   * a post takes the place, a retraction gives it back, a reaction or a
   * membership event leaves it; `null` when nothing stands. Absent on a
   * snapshot frame, which names a definition that moved and no words.
   */
  latest?: import("./types.gen").Representative | null;
  snapshot?: boolean;
}

/**
 * One conversation's inbox state, emitted beside the frame that changed it.
 *
 * This is what a row is patched *with*. The alternative — refetching the list
 * and replacing the array — rebuilds every row on every message, which is how
 * a selection ends up under somebody's cursor mid-read.
 *
 * Source: `crates/bisa-node/src/inbox.rs::delta`.
 */
export interface InboxFrame {
  key: string;
  /** The kind of thing the key names; absent when the node could not tell. */
  kind?: import("./types.gen").InboxKind | null;
  unread_count: number;
  needs_action_count: number;
  latest_at: number;
  read: boolean;
  handled: boolean;
  /** Ride only when the cause could have moved the notices — an engine fact, a read marker; a message leaves them out. */
  notice_count?: number;
  unread_notices?: number;
  /** A `session` row's frame: whether the harness still waits in its terminal — false drops the row. */
  waiting?: boolean;
}

export type { ConversationKind };

/**
 * What the shell did with the files pasted from the file manager
 * (`src-tauri/src/pasteboard.rs::PasteReport`): each entry that landed with
 * the name it landed under, and each left out with why.
 */
export interface PasteReport {
  pasted: Pasted[];
  skipped: { path: string; why: string }[];
}

/** One entry pasted: where it came from and the name it landed under (`pasteboard.rs::Pasted`). */
export interface Pasted {
  from: string;
  name: string;
}

/**
 * What the machine's clipboard holds that a paste can take
 * (`src-tauri/src/pasteboard.rs::Holdings`): the files a copy in the file
 * manager put there, and whether a picture is there.
 */
export interface PasteboardHoldings {
  paths: string[];
  image: boolean;
}

/**
 * How a model plan reads in one line:
 * `claude-opus-5-5[1m] → claude-sonnet-5-5[1m]`, or the honest
 * "harness default" when it is empty. The engine's own ordering is richer
 * (it skips models in cooldown); this is the definition, not the live pick.
 */
export function planSummary(plan: ModelPlan | null | undefined): string {
  const models = (plan?.models ?? []).filter((m) => m.enabled);
  if (models.length === 0) return t("app-types-harness-default");
  return models.map((m) => m.model).join(" → ");
}

// ---------------------------------------------------------------------------
// Projects and workstreams — response envelopes assembled with `json!` in
// `bisa-node/src/projects.rs` (the request bodies and the row types are
// generated; these wrappers are not)
// ---------------------------------------------------------------------------

/** `GET /projects/{pid}` */
export interface ProjectDetail {
  project: Project;
  /** Resolved root: an adopted project shows its real path. */
  path: string;
  exists: boolean;
  /** Goals this project is attached to — zero or more, none of them an owner. */
  goals: string[];
  workstreams: Workstream[];
}

/** `GET /workstreams/{wid}/git/status` — neutral, never an error, for a plain folder. */
export interface WorkstreamGitStatus {
  workstream: string;
  project: string;
  path: string;
  publish: Project["publish"];
  default_branch: string | null;
  status: GitStatusInfo;
}

/**
 * `GET /workstreams/{wid}/git/files` — every path git has something to say
 * about, with both of its letters.
 *
 * A read and only a read: it stages nothing, which is why an untracked file
 * arrives here as a row and *not* as a patch. `clean` is the whole tree, not
 * the index.
 */
export interface WorkstreamGitFiles {
  workstream: string;
  clean: boolean;
  files: GitFileRow[];
}

/**
 * `GET /workstreams/{wid}/git/diff?path=&staged=` — one file's patch.
 *
 * `untracked: true` comes with an **empty `diff`**, and that is the honest
 * answer rather than a failure: `git diff` cannot show a file git has never
 * seen without staging it first, and a GET does not stage. The desktop then
 * reads the file itself and draws it as the all-additions patch git would
 * write for it (`views/_work/newFilePatch.mjs`), read-only. `truncated` says
 * the patch was cut at the node's cap.
 *
 * `file` is the row as `git/files` would report it now, so a caller that
 * arrived here from a stale list can correct itself; it is null when the path
 * has stopped being interesting to git at all.
 */
export interface WorkstreamFileDiff {
  workstream: string;
  path: string;
  staged: boolean;
  diff: string;
  truncated: boolean;
  untracked: boolean;
  file: GitFileRow | null;
}

/**
 * One line of `GET /workstreams/{wid}/git/blame`: who last touched it. An
 * `uncommitted` line is one the working tree has that HEAD does not; its
 * `timestamp` is 0 and its `summary` empty.
 */
export interface BlameLine {
  line: number;
  commit: string;
  short: string;
  author: string;
  timestamp: number;
  summary: string;
  uncommitted: boolean;
}

/** A decoration on a commit: a branch, a remote branch, a tag, or `HEAD`. */
interface GraphRef {
  name: string;
  kind: "head" | "branch" | "remote" | "tag" | string;
}

export interface GraphEdge {
  from: number;
  to: number;
  /** `fork`: this commit's extra parent sits in lane `to`; `merge`: lane `from` converged here. */
  kind: "fork" | "merge";
}

/** One row of `GET /ide/graph/{scope}/{id}` — a commit with its lane and edges. */
export interface GraphRow {
  id: string;
  short: string;
  lane: number;
  /** Lanes that continue below this row; the row above's `passing` enters from the top. */
  passing: number[];
  edges: GraphEdge[];
  refs: GraphRef[];
  author: string;
  timestamp: number;
  subject: string;
  parents: number;
}

/**
 * A window of the commit graph. `done` is false while only the first screen
 * is laid out; `stale` while the repository moved on and a relayout runs —
 * the rows shown are the last good ones, and the next request sees the new.
 */
export interface GraphWindow {
  total: number;
  done: boolean;
  stale: boolean;
  from: number;
  rows: GraphRow[];
}

/**
 * `GET /ide/graph/{scope}/{id}/search`: the row indexes matching a query
 * across the whole laid-out log — not the page the client has loaded.
 */
/** The graph's one filter (ide/05): every local branch and tag, or only what HEAD reaches. */
export type GraphRefScope = "all" | "head";

export interface GraphMatches {
  /** Row indexes, ascending. */
  indices: number[];
  /** How many rows were searched — the whole log once `done`. */
  searched: number;
  done: boolean;
  /** The cap stopped the search; more rows match below the last index. */
  truncated: boolean;
}

/** One hit of `GET /ide/search/{scope}/{id}`: a line in a file, with a line of context either side. */
export interface SearchHit {
  path: string;
  /** 1-based. */
  line: number;
  /** 1-based byte column of the match. */
  column: number;
  text: string;
  before: string | null;
  after: string | null;
}

/** The `done` frame of a content search. */
export interface SearchSummary {
  matches: number;
  files_with_matches: number;
  files_scanned: number;
  /** The hit cap stopped the walk; narrow the query. */
  truncated: boolean;
}

/** What a content search asks for; mirrors `engine::ide::search::SearchQuery`. */
export interface SearchParams {
  q: string;
  regex?: boolean;
  case?: "smart" | "sensitive" | "insensitive";
  word?: boolean;
  include?: string[];
  exclude?: string[];
  limit?: number;
}

/** One file of a commit, against its first parent. */
interface CommitFileChange {
  path: string;
  old_path: string | null;
  kind: string;
  insertions: number;
  deletions: number;
  binary: boolean;
}

/** `GET /workstreams/{wid}/git/commit/{sha}` — one commit in full. */
interface CommitDetail {
  id: string;
  short: string;
  parents: string[];
  refs: GraphRef[];
  author: string;
  email: string;
  timestamp: number;
  subject: string;
  body: string;
  files: CommitFileChange[];
}

export interface WorkstreamCommitView {
  workstream: string;
  commit: CommitDetail;
  diff: string;
  /** The patch was cut at 2 MiB. */
  truncated: boolean;
}

/**
 * `GET /workstreams/{wid}/status` and the batched forms — what a switcher row
 * shows (ide/07): branch, distance from base and upstream, dirty counts, the
 * agents running in it. Cached two seconds per workstream on the node.
 */
export interface WorkstreamStatus {
  workstream: string;
  project: string;
  kind: WorkstreamKind;
  /** The person's label for it, when they gave one. */
  name: string | null;
  publish: Project["publish"];
  default_branch: string | null;
  exists: boolean;
  git: boolean;
  branch: string | null;
  base: string | null;
  ahead_of_base: number | null;
  behind_base: number | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
  clean: boolean;
  /** A merge, rebase, cherry-pick or revert git has left half-done here. */
  in_progress: GitInProgress | null;
  /** The pull request the record knows of, when one is open. */
  pr: { number: number; url: string } | null;
  running_agents: number;
  state: WorkstreamState;
}

/** How a pull moves the branch once the fetch has landed. */
export type PullMode = "ff_only" | "rebase" | "merge";

/** What `POST /workstreams/{wid}/git/pull` did. */
export interface PullOutcome {
  mode: PullMode;
  upstream: string;
  from: string;
  to: string;
  moved: boolean;
}

/** One row of `GET /ide/lsp/{scope}/{id}/status`. */
export interface LspServerStatus {
  language: string;
  command: string;
  /** On the login shell's PATH. */
  available: boolean;
  install_hint?: string | null;
  state: { state: "starting" | "running" | "stopped" } | { state: "failed"; reason: string };
  documents: number;
}

/** One local branch, from `GET /workstreams/{wid}/git/branches`, with its standing. */
export interface BranchInfo {
  name: string;
  head: string;
  current: boolean;
  upstream?: string | null;
  subject: string;
  timestamp: number;
  /** Commits it has that its upstream lacks; 0 without one, or when the upstream is gone. */
  ahead: number;
  behind: number;
  /** Reachable from the project's default branch. */
  merged: boolean;
}

/** One remote-tracking branch, from `GET /workstreams/{wid}/git/remote-branches`. */
export interface RemoteBranchInfo {
  remote: string;
  /** The branch's name on the remote, without the remote's prefix. */
  name: string;
  head: string;
  subject: string;
  timestamp: number;
}

/**
 * Where a workstream starts (ide/07 §Where a workstream starts) — the
 * `source` of `POST /projects/{pid}/workstreams`, tagged by its `source` word.
 */
export interface TagInfo {
  name: string;
  /** The commit the tag points at, peeled for annotated tags. */
  target: string;
  subject: string;
  timestamp: number;
}

export interface RemoteInfo {
  name: string;
  url: string;
}

/** What a consented operation saved first. */
export interface Recovery {
  ref_name: string;
  commit: string;
  was_clean: boolean;
  branch?: string | null;
}

/**
 * The answer of every consented git route: the recovery ref written before
 * the operation ran, the branch HEAD is on now (`null` when detached), and
 * the fresh file rows.
 */
export interface GitDone {
  workstream: string;
  recovery: Recovery;
  branch: string | null;
  files: GitFileRow[];
}

/** `GET /workstreams/{wid}/git/blame?path=` */
export interface WorkstreamBlame {
  workstream: string;
  path: string;
  lines: BlameLine[];
}

/** One commit of `GET /workstreams/{wid}/git/history` — newest first. */
/** One commit as the safe tier's `log` shape reports it — a file's history, a branch's commits since another. */
export interface CommitSummary {
  id: string;
  short: string;
  author: string;
  email: string;
  /** Author time, seconds since the epoch. */
  timestamp: number;
  subject: string;
}

/** `GET /workstreams/{wid}/git/history?path=` — follows renames. */
export interface WorkstreamHistory {
  workstream: string;
  path: string;
  commits: CommitSummary[];
}

/** `GET /projects/{pid}/review` — oldest first; resolved notes only on request. */
export interface ProjectReviewNotes {
  project: string;
  notes: ReviewNote[];
}

/**
 * `POST /projects/{pid}/review/send` — the notes now marked sent, and the goal
 * threads a message landed in. `posted_to` is empty for a project attached to
 * no goal: the notes are still sent, and an agent working in the project
 * reads them with `review_notes_list`.
 */
export interface ReviewNotesSent {
  project: string;
  notes: ReviewNote[];
  posted_to: string[];
}

/**
 * `POST /workstreams/{wid}/git/stage` and `/git/unstage` — the fresh file list,
 * so the caller refreshes from its own response rather than re-reading.
 */
export interface WorkstreamStaged {
  workstream: string;
  files: GitFileRow[];
}

/**
 * `POST /workstreams/{wid}/git/commit` — **409** on a clean tree or a selection
 * that matched nothing changed. No Publish gate: a commit is local and the
 * user's own git can undo it.
 */
export interface WorkstreamCommitted {
  workstream: string;
  commit: string;
  short: string;
  files: GitFileRow[];
}

/**
 * `POST /workstreams/{wid}/git/amend` — the last commit rewritten; consented,
 * the old commit pinned in Safety first (`recovery`), the new one named.
 */
export type WorkstreamAmended = GitDone & { commit: string; short: string };

/**
 * `POST /workstreams/{wid}/git/message` — **always 200.**
 *
 * `suggested: false` with an `error` is the no-harness / timeout case, and it
 * must leave the user with an empty box and a sentence rather than a message
 * nobody wrote. The session behind it runs read-only with no MCP servers: the
 * thing that suggests a commit cannot make one.
 */
export interface SuggestedCommitMessage {
  workstream: string;
  suggested: boolean;
  message: string;
  /** The core agent's id — the same one every time, named rather than implied. */
  agent: string;
  error: string | null;
}

/**
 * The notes repository's answer to *Suggest*: the same shape without a
 * workstream, since notes belong to none. Read by `gitFiles.suggestionOutcome`
 * unchanged.
 */
export type SuggestedNotesMessage = Omit<SuggestedCommitMessage, "workstream">;

/**
 * `POST /workstreams/{wid}/pr/suggest` — a pull request's title and body,
 * drafted by the core agent from the branch's commits and its diff against
 * the base. The same contract as {@link SuggestedCommitMessage}: always 200,
 * `suggested: false` with an `error` when nothing was drafted (read by
 * `prFormModel.prSuggestionOutcome`), and the session that drafts it cannot
 * push or open anything.
 */
export interface SuggestedPullRequest {
  workstream: string;
  suggested: boolean;
  title: string;
  body: string;
  /** The core agent's id — the same one every time, named rather than implied. */
  agent: string;
  error: string | null;
}

/** `POST /projects` and `POST /goals/{id}/projects` */
export interface ProjectCreated {
  project: Project;
  path: string;
  exists: boolean;
  /** Goals the project is attached to after this call. */
  goals: string[];
  /** The goal the route attached it to, when it was `POST /goals/{id}/projects`. */
  attached_to?: string | null;
  /**
   * Present only for `{"kind": "import"}`: what the copy moved, and what it
   * left behind. Symlinks are skipped rather than followed — following one
   * would copy whatever it points at, which may be nowhere near the folder
   * being imported — so a tree that leaned on them arrives incomplete and
   * should say so.
   */
  imported?: ImportStats | null;
}

/** The counts `{"kind": "import"}` reports back. */
export interface ImportStats {
  files: number;
  dirs: number;
  bytes: number;
  skipped_symlinks: number;
  skipped_special: number;
}

/** `GET /workstreams/{wid}` */
export interface WorkstreamDetail {
  workstream: Workstream;
  /** Where the checkout is — the project root for the primary. */
  path: string;
  project: Project;
  branch: string | null;
  base: string | null;
  /** Commits this branch has that its base does not. */
  ahead_of_base: number | null;
  behind_base: number | null;
  status: GitStatusInfo;
  changes: ChangedFile[];
}

/**
 * `GET /workstreams/{wid}/diff` — what is not committed yet.
 *
 * `diff` and `changes` cover tracked changes only, because `git diff` cannot
 * show a brand-new file without staging it and a read must not stage. New
 * files arrive in `untracked`, and `clean` counts them.
 */
export interface WorkstreamDiff {
  workstream: string;
  clean: boolean;
  diff: string;
  changes: ChangedFile[];
  untracked: string[];
}

export interface CommitResult {
  workstream: Workstream;
  commit: string;
  short: string;
}

/**
 * `POST /workstreams/{wid}/push` — the Publish gate, expressed over HTTP.
 *
 * A gated project answers **202** with the gate id and nothing has left the
 * machine: the push completes when a human decides the gate (from the inbox,
 * `decide`, or the CLI). `PublishPolicy::Auto` answers 200 with it already
 * done, and `Manual` is a 409 error rather than either.
 */
export type PushOutcome =
  | { status: "awaiting_publish_gate"; workstream: string; gate: string; pushed: false }
  | { status: "pushed"; workstream: Workstream; pushed: true };

/** `POST /workstreams/{wid}/pr` — the same three outcomes as {@link PushOutcome}. */
export type PrOutcome =
  | { status: "awaiting_publish_gate"; workstream: string; gate: string; opened: false }
  | {
      status: "pr_open";
      workstream: Workstream;
      opened: true;
      pr: { number: number; url: string };
    };

/**
 * Reviewing an agent's changes (ide/20), in the desktop's own words. The
 * schema names these after their Rust types — `ConversationAsk`, `Target`,
 * `ChangesSettled` — and the routes' envelopes derive no schema.
 */
export type AskView = import("./types.gen").ConversationAsk;
/** `PATCH /teams/{id}`: what is absent is kept; `photo: null` removes the picture. */
export type TeamPatchBody = import("./types.gen").TeamPatch;

/** `POST /goals/{id}/close`: a rationale, or the goal that replaces this one. */
export type CloseGoalBody = import("./types.gen").CloseBody;

export type AnswerAskBody = import("./types.gen").ConversationAskAnswer;
export type SettleTarget = import("./types.gen").Target;
export type Settled = import("./types.gen").ChangesSettled;

/** `GET /conversations/{id}/asks`, and the answer to `POST /conversations/{id}/asks/{ask}`. */
export interface AsksResponse {
  asks: AskView[];
}
