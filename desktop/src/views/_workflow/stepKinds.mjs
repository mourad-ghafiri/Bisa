/**
 * The designer's vocabulary: the eighteen step kinds in their four families
 * — events, gateways, loops, tasks — their blank shapes, and the closed sets
 * the forms choose from.
 *
 * Plain `.mjs` with a `.d.mts` beside it, like every model in this app:
 * `node --test` imports it with no build step. The guard test beside it
 * reads `StepKind` (its names, their order and their families), `Condition`,
 * `WaitFor`, `CheckKind`, `Join`, `OnFail`, `Pick`, `Finish` and
 * `ProblemKind` out of `crates/bisa-core/src/workflow.rs`, `StartOn`,
 * `Overlap` and `FireOn` out of `start.rs`, `BoundaryOn`, `BoundaryAct` and
 * `may_carry_boundaries` out of `boundary.rs`, `MessageFrom`,
 * `ProjectChange` and `RunEnd` out of `listen.rs`, and `StepState` and
 * `RunOutcome` out of `run.rs`, so a variant added in Rust fails here before
 * it fails on a canvas.
 *
 * Defaults are the core's (`D2` in the design): `max_visits` 3, `join` all,
 * `on_fail` fail, `retries` 0, an agent step's `tier_ceiling` write.
 */

import { t } from "../../i18n/l10n.mjs";

/** The palette's groups, in the order it draws them: what happens, what routes, what repeats, what works. */
export const FAMILIES = Object.freeze(["event", "gateway", "loop", "task"]);

/** Each group's heading. */
export const FAMILY_LABEL = Object.freeze({
  event: t("workflow-step-kinds-events"),
  gateway: t("workflow-step-kinds-gateways"),
  loop: t("workflow-step-kinds-loops"),
  task: t("workflow-step-kinds-tasks"),
});

/** The palette, family by family, in the order a designer reaches for them — the core's `StepKind::NAMES`. */
export const STEP_KINDS = [
  { kind: "start", family: "event", label: t("workflow-step-kinds-start"), explain: t("workflow-step-kinds-one-way-run-begins-hand-event") },
  { kind: "wait", family: "event", label: t("workflow-step-kinds-wait"), explain: t("workflow-step-kinds-hold-until-world-moves") },
  { kind: "emit", family: "event", label: t("workflow-step-kinds-emit-signal"), explain: t("workflow-step-kinds-raise-named-signal-workflows-start-wait") },
  { kind: "end", family: "event", label: t("workflow-step-kinds-end"), explain: t("workflow-step-kinds-end-path-finish-run-fail-it") },
  { kind: "decide", family: "gateway", label: t("workflow-step-kinds-decide"), explain: t("workflow-step-kinds-first-every-rule-holds-names-branches") },
  { kind: "if", family: "gateway", label: t("workflow-step-kinds-if"), explain: t("workflow-step-kinds-one-condition-yes-when-holds-no") },
  { kind: "switch", family: "gateway", label: t("workflow-step-kinds-switch"), explain: t("workflow-step-kinds-rendered-value-against-cases-otherwise-when") },
  { kind: "judge", family: "gateway", label: t("workflow-step-kinds-judge"), explain: t("workflow-step-kinds-decision-making-agent-reads-rendered-state-picks") },
  { kind: "parallel", family: "gateway", label: t("workflow-step-kinds-parallel"), explain: t("workflow-step-kinds-every-flow-out-at-once-paths-meet") },
  { kind: "for_each", family: "loop", label: t("workflow-step-kinds-each"), explain: t("workflow-step-kinds-one-item-time-down-each-flow") },
  { kind: "while", family: "loop", label: t("workflow-step-kinds-while"), explain: t("workflow-step-kinds-round-loop-flow-while-condition-holds") },
  { kind: "agent", family: "task", label: t("workflow-step-kinds-agent"), explain: t("workflow-step-kinds-agent-does-work-yields-result") },
  { kind: "human", family: "task", label: t("workflow-step-kinds-human"), explain: t("workflow-step-kinds-person-answers-does-something-hand-marks") },
  { kind: "approval", family: "task", label: t("workflow-step-kinds-approval"), explain: t("workflow-step-kinds-signed-yes-no-declined-failure-step") },
  { kind: "check", family: "task", label: t("workflow-step-kinds-check"), explain: t("workflow-step-kinds-command-schema-judges-result-not-passing") },
  { kind: "connector", family: "task", label: t("workflow-step-kinds-connector"), explain: t("workflow-step-kinds-call-one-operation-outside-platform-through") },
  { kind: "notify", family: "task", label: t("workflow-step-kinds-notify"), explain: t("workflow-step-kinds-post-into-conversation-agent-workflow-agent") },
  { kind: "spawn", family: "task", label: t("workflow-step-kinds-spawn"), explain: t("workflow-step-kinds-sub-goal-own-workflow-linked-one") },
];

/** A kind's family — `task` for a kind this build does not know. */
export function familyOf(kind) {
  return STEP_KINDS.find((k) => k.kind === kind)?.family ?? "task";
}

/** A kind's palette label, the kind's own word for one this build does not know. */
export function kindLabel(kind) {
  return STEP_KINDS.find((k) => k.kind === kind)?.label ?? kind;
}

/**
 * The kinds whose work can be stopped while it is live — the only ones that
 * may carry boundary events: an agent's session, a person's answer or
 * approval, a wait, a spawned goal the step waits for. The core's
 * `may_carry_boundaries`.
 */
export function mayCarryBoundaries(step) {
  switch (step?.kind) {
    case "agent":
    case "human":
    case "approval":
    case "wait":
      return true;
    case "spawn":
      return step.wait !== false;
    default:
      return false;
  }
}

/** The tests a `decide` rule, an `if` or a `while` may use; the last four combine the others. No test reads the event: a start maps it onto inputs, and a rule reads those. */
export const CONDITIONS = [
  { condition: "input_equals", label: t("workflow-step-kinds-input-equals") },
  { condition: "output_equals", label: t("workflow-step-kinds-step-s-output-equals") },
  { condition: "output_matches", label: t("workflow-step-kinds-step-s-output-contains") },
  { condition: "answered", label: t("workflow-step-kinds-human-step-answered") },
  { condition: "outcome", label: t("workflow-step-kinds-check-approval-passed") },
  { condition: "between", label: t("workflow-step-kinds-hour-utc-between") },
  { condition: "all", label: t("workflow-step-kinds-all-these-hold") },
  { condition: "any", label: t("workflow-step-kinds-any-these-holds") },
  { condition: "one", label: t("workflow-step-kinds-exactly-one-these-holds") },
  { condition: "not", label: t("workflow-step-kinds-does-not-hold") },
];

/** The conditions that hold other conditions; the rest are leaves. */
export const COMBINATORS = ["all", "any", "one", "not"];

/** How deep conditions may nest — the core's `Condition::MAX_DEPTH`. */
export const MAX_CONDITION_DEPTH = 8;

/** What a `wait` step holds for — the catch events. */
export const WAITS = [
  { until: "delay", label: t("workflow-step-kinds-number-seconds") },
  { until: "time", label: t("workflow-step-kinds-moment-deadline") },
  { until: "schedule", label: t("workflow-step-kinds-cron-schedule") },
  { until: "signal", label: t("workflow-step-kinds-named-signal") },
  { until: "message", label: t("workflow-step-kinds-message") },
  { until: "project", label: t("workflow-step-kinds-project-s-change") },
  { until: "run", label: t("workflow-step-kinds-run-s-end") },
  { until: "platform", label: t("workflow-step-kinds-platform-event") },
  { until: "release", label: t("workflow-step-kinds-person-releasing") },
];

/**
 * A fresh catch of one kind with the fields it needs. A reference not chosen
 * yet — a project — is `null`, never `""`.
 */
export function blankWait(until) {
  switch (until) {
    case "delay":
      return { until, secs: 3600 };
    case "time":
      return { until, at: "" };
    case "schedule":
      return { until, cron: "0 9 * * 1-5", tz: null };
    case "signal":
      return { until, name: "", fields: {} };
    case "message":
      return { until };
    case "project":
      return { until, project: null, change: "commit" };
    case "run":
      return { until };
    case "platform":
      return { until, topic: "" };
    case "release":
      return { until };
    default:
      throw new Error(`not a catch: ${until}`);
  }
}

/** What begins a run at a `start` step, in the words the form offers. */
export const START_EVENTS = [
  { event: "manual", label: t("workflow-step-kinds-by-hand") },
  { event: "schedule", label: t("workflow-step-kinds-on-schedule") },
  { event: "hook", label: t("workflow-step-kinds-when-called") },
  { event: "message", label: t("workflow-step-kinds-when-message-arrives") },
  { event: "signal", label: t("workflow-step-kinds-when-signal-raised") },
  { event: "project", label: t("workflow-step-kinds-when-project-changes") },
  { event: "run", label: t("workflow-step-kinds-when-run-finishes") },
  { event: "platform", label: t("workflow-step-kinds-when-platform-says") },
  { event: "connector", label: t("workflow-step-kinds-when-outside-platform-lists-something-new") },
  { event: "check", label: t("workflow-step-kinds-when-check-starts-failing") },
];

/** What a boundary event listens for while its step is live. */
export const BOUNDARY_EVENTS = [
  { event: "after", label: t("workflow-step-kinds-timeout") },
  { event: "every", label: t("workflow-step-kinds-reminder") },
  { event: "message", label: t("workflow-step-kinds-boundary-message") },
  { event: "signal", label: t("workflow-step-kinds-boundary-signal") },
];

/** What a boundary event does when heard: stop the step and take its path, or act beside it. */
export const BOUNDARY_ACTS = [
  { act: "divert", label: t("workflow-step-kinds-divert-path") },
  { act: "notify", label: t("workflow-step-kinds-post") },
  { act: "emit", label: t("workflow-step-kinds-emit") },
];

/** What an `end` step ends. */
export const END_FINISHES = [
  { finish: "path", label: t("workflow-step-kinds-end-this-path") },
  { finish: "done", label: t("workflow-step-kinds-finish-run") },
  { finish: "failed", label: t("workflow-step-kinds-fail-run-now") },
];

/** Which of a `decide`'s rules choose. */
export const DECIDE_PICKS = [
  { pick: "first", label: t("workflow-step-kinds-first-rule-holds") },
  { pick: "every", label: t("workflow-step-kinds-every-rule-holds") },
];

/** What an occurrence does while a run its start began is still going — the guard's overlap, in words. */
export const OVERLAPS = [
  { overlap: "queue", label: t("workflow-step-kinds-one-at-time-later-wait") },
  { overlap: "skip", label: t("workflow-step-kinds-skip-while-one-runs") },
  { overlap: "parallel", label: t("workflow-step-kinds-several-at-once") },
];

/** Who wrote a message a message event waits for. `someone` names one agent, person or team. */
export const MESSAGE_FROM = [
  { from: "you", label: t("workflow-step-kinds-from-you") },
  { from: "agents", label: t("workflow-step-kinds-from-any-agent") },
  { from: "someone", label: t("workflow-step-kinds-from-someone-named") },
];

/** What changed in a project. */
export const PROJECT_CHANGES = [
  { change: "commit", label: t("workflow-step-kinds-commit") },
  { change: "push", label: t("workflow-step-kinds-push-fetch") },
  { change: "pull_request", label: t("workflow-step-kinds-pull-request-s-change") },
  { change: "merge", label: t("workflow-step-kinds-merge") },
  { change: "files", label: t("workflow-step-kinds-change-files") },
];

/** How a run ended, as a run event hears it; absent, any end. */
export const RUN_ENDS = [
  { outcome: "done", label: t("workflow-step-kinds-run-done") },
  { outcome: "failed", label: t("workflow-step-kinds-run-failed") },
  { outcome: "cancelled", label: t("workflow-step-kinds-run-cancelled") },
];

/** Which results of a check start begin a run. */
export const FIRE_ON = [
  { fire_on: "starts_failing", label: t("workflow-step-kinds-when-starts-failing") },
  { fire_on: "failing", label: t("workflow-step-kinds-every-time-fails") },
  { fire_on: "passing", label: t("workflow-step-kinds-every-time-passes") },
  { fire_on: "always", label: t("workflow-step-kinds-every-time-runs") },
];

/** How a `check` step judges. */
export const CHECKS = [
  { check: "command", label: t("workflow-step-kinds-shell-command-exits-0") },
  { check: "schema", label: t("workflow-step-kinds-upstream-output-fits-json-schema") },
];

export const JOINS = [
  { join: "all", label: t("workflow-step-kinds-wait-every-incoming-flow") },
  { join: "any", label: t("workflow-step-kinds-start-first-arrival") },
  { join: "one", label: t("workflow-step-kinds-exactly-one-arrives-second-fails-step") },
];

export const ON_FAILS = [
  { on_fail: "fail", label: t("workflow-step-kinds-fail-run") },
  { on_fail: "skip", label: t("workflow-step-kinds-skip-continue") },
  { on_fail: "then", label: t("workflow-step-kinds-route-another-step") },
];

/**
 * The one sentence every template field's hint ends with: the grammar the
 * engine's `template.rs` reads, and how to write a brace that is not a
 * placeholder. A step never reads the event — its start maps it onto inputs.
 */
export const TEMPLATE_HINT =
  t("workflow-step-kinds-placeholders-inputs-x-steps-id-output");

/** The problems a definition can have, in words a designer can act on. */
export const PROBLEM_KIND_LABEL = {
  empty_name: t("workflow-step-kinds-needs-name"),
  duplicate_step_id: t("workflow-step-kinds-two-steps-share-id"),
  duplicate_input: t("workflow-step-kinds-two-inputs-share-name"),
  no_start: t("workflow-step-kinds-workflow-has-no-steps"),
  many_starts: t("workflow-step-kinds-more-than-one-step-has-nothing"),
  unknown_step: t("workflow-step-kinds-flows-step-does-not-exist"),
  unreachable: t("workflow-step-kinds-nothing-reaches-step"),
  self_flow: t("workflow-step-kinds-flows-into-itself"),
  branch_without_rule: t("workflow-step-kinds-labelled-flow-names-branch-no-rule"),
  rule_without_flow: t("workflow-step-kinds-rule-chooses-branch-no-flow-carries"),
  labelled_flow_on_plain_step: t("workflow-step-kinds-only-branching-step-decide-if-switch"),
  unlabelled_flow_on_decide: t("workflow-step-kinds-every-flow-out-branching-step-carries"),
  duplicate_branch: t("workflow-step-kinds-two-flows-carry-same-branch"),
  not_upstream: t("workflow-step-kinds-refers-step-does-not-run-before"),
  unknown_input: t("workflow-step-kinds-names-input-workflow-does-not-declare"),
  input_kind_mismatch: t("workflow-step-kinds-input-not-kind-field-needs"),
  unknown_placeholder: t("workflow-step-kinds-placeholder-root-platform-does-not-know"),
  bad_template: t("workflow-step-kinds-template-s-braces-do-not-balance"),
  zero_visits: t("workflow-step-kinds-max-visits-must-least-one"),
  unknown_assignee: t("workflow-step-kinds-names-agent-team-person-not-here"),
  unknown_workflow: t("workflow-step-kinds-names-workflow-not-installed"),
  unknown_project: t("workflow-step-kinds-names-project-does-not-exist"),
  end_with_successors: t("workflow-step-kinds-end-step-has-flows-out"),
  bad_question: t("workflow-step-kinds-not-valid-question-human-step-s"),
  unused_input: t("workflow-step-kinds-input-declared-but-no-step-reads"),
  empty_rules: t("workflow-step-kinds-nothing-choose-between-so-always-takes"),
  unknown_harness: t("workflow-step-kinds-names-harness-runtime-cannot-launch"),
  unknown_model: t("workflow-step-kinds-pins-model-harness-does-not-list"),
  bad_cron: t("workflow-step-kinds-cron-expression-cannot-scheduled"),
  bad_schema: t("workflow-step-kinds-schema-not-json-schema"),
  spawn_cycle: t("workflow-step-kinds-spawns-workflow-spawns-one-back"),
  notify_scope_unknown: t("workflow-step-kinds-posts-into-something-not-channel-goal"),
  notify_author_not_an_agent: t("workflow-step-kinds-speaks-person-team-only-agent-speaks"),
  bad_condition: t("workflow-step-kinds-condition-empty-nests-too-deep"),
  loop_without_return: t("workflow-step-kinds-nothing-loop-s-body-side-flows"),
  loop_exit_returns: t("workflow-step-kinds-loop-s-done-flow-leads-back"),
  zero_iterations: t("workflow-step-kinds-max-iterations-must-least-one"),
  unknown_connector: t("workflow-step-kinds-calls-connector-not-installed-here"),
  unknown_operation: t("workflow-step-kinds-calls-operation-connector-does-not-have"),
  missing_connector_param: t("workflow-step-kinds-parameter-unset-not-one-operation-takes"),
  unknown_account: t("workflow-step-kinds-runs-account-machine-does-not-have"),
  ungated_write: t("workflow-step-kinds-writes-platform-no-approval-human-step"),
  param_only_placeholder: t("workflow-step-kinds-params-account-placeholder-belongs-connector-definition"),
  unfilled: t("workflow-step-kinds-choice-not-made-yet-pick-inspector"),
  not_assured: t("workflow-step-kinds-reads-step-not-sure-have-run"),
  no_such_output: t("workflow-step-kinds-reads-output-from-step-yields-none"),
  unpromised_output: t("workflow-step-kinds-reads-field-step-s-output-schema"),
  needs_goal: t("workflow-step-kinds-reads-goal-run-workspace-has-none"),
  start_has_incoming: t("workflow-step-kinds-flow-leads-into-start"),
  many_manual_starts: t("workflow-step-kinds-more-than-one-start-by-hand"),
  manual_start_configured: t("workflow-step-kinds-start-by-hand-carries-mapping-guard"),
  start_placeholder: t("workflow-step-kinds-event-read-outside-start-s-inputs"),
  boundary_on_instant_step: t("workflow-step-kinds-boundary-events-step-cannot-stopped"),
  reminder_interrupts: t("workflow-step-kinds-reminder-diverts-stops-step-first-tick"),
  bad_timer: t("workflow-step-kinds-clock-cannot-run"),
  bad_signal_name: t("workflow-step-kinds-signal-name-not-dotted-lowercase-words"),
  bad_poll: t("workflow-step-kinds-poll-cannot-poll"),
  unknown_topic: t("workflow-step-kinds-platform-event-never-emitted"),
  spawn_needs_manual_entry: t("workflow-step-kinds-spawns-workflow-only-events-start"),
  unsupported_effort: t("workflow-step-kinds-pins-effort-none-harnesses-can-set"),
  spawn_input: t("workflow-step-kinds-spawn-gives-not-what-workflow-asks"),
};

/** The MIME type a palette item carries when dragged onto the canvas. */
export const STEP_MIME = "application/x-bisa-step";

/**
 * The handle a branch's flows leave from — a gateway's branch, a loop's
 * body or exit, a divert boundary's path. Prefixed, so a branch may be
 * named anything — `out`, `fail`, `loop-out` included — without colliding
 * with the handle ids every step shares.
 */
export const BRANCH_HANDLE_PREFIX = "branch:";

export function branchHandle(branch) {
  return `${BRANCH_HANDLE_PREFIX}${branch}`;
}

/** The branch a handle id names, or `null` for a handle that is not a branch's. */
export function branchOfHandle(handle) {
  return typeof handle === "string" && handle.startsWith(BRANCH_HANDLE_PREFIX) ? handle.slice(BRANCH_HANDLE_PREFIX.length) : null;
}

export const DEFAULT_MAX_VISITS = 3;

/** A loop step's bound on its iterations — the core's `DEFAULT_MAX_ITERATIONS`. */
export const DEFAULT_MAX_ITERATIONS = 100;

/**
 * The branch words a kind's flows carry when the kind, not the author,
 * names them: an `if`'s answer, a loop's body and exit. The core's `branch`
 * module spells the same words.
 */
export const FIXED_BRANCHES = Object.freeze({
  if: ["yes", "no"],
  for_each: ["each", "done"],
  while: ["loop", "done"],
});

/**
 * The condition a new `decide` rule, `if` or `while` starts with: an empty
 * group. A leaf that names a step is meaningless without one, and a
 * reference is a real id or absent — never an empty string — so the blank is
 * the one condition that names nothing; the validator calls it a problem
 * (*add a condition*) until the editor fills it.
 */
export function blankCondition() {
  return { condition: "all", of: [] };
}

/** An option id no option of a `human` step has yet. */
export function freshOptionId(options) {
  const taken = new Set(options.map((o) => o.id));
  for (let n = 1; ; n++) {
    const id = `option-${n}`;
    if (!taken.has(id)) return id;
  }
}

/**
 * A fresh step of one kind, with the core's defaults and an id the caller
 * chose. The per-kind fields are the minimum a valid step needs; the forms
 * fill the rest. A reference the person has not chosen yet — a connector,
 * its operation — is `null`, never `""`: the node reads `null` as a choice
 * not made (a problem it lists) and `""` as an id it cannot parse (a body it
 * refuses). A new `start` begins by hand; an `end` ends its path.
 */
export function blankStep(kind, id) {
  const base = {
    id,
    name: STEP_KINDS.find((k) => k.kind === kind)?.label ?? kind,
    then: [],
    join: "all",
    on_fail: { on_fail: "fail" },
    retries: 0,
    max_visits: DEFAULT_MAX_VISITS,
  };
  switch (kind) {
    case "start":
      return { ...base, kind, on: { event: "manual" } };
    case "agent":
      return { ...base, kind, instructions: "", harness: [], tier_ceiling: "write" };
    case "human":
      return { ...base, kind, prompt: "", options: [], multi: false };
    case "approval":
      return { ...base, kind, prompt: "" };
    case "check":
      return { ...base, kind, check: { check: "command", command: "" } };
    case "decide":
      return { ...base, kind, rules: [], otherwise: "otherwise" };
    case "if":
      return { ...base, kind, when: blankCondition() };
    case "switch":
      return { ...base, kind, on: "", cases: [], otherwise: "otherwise" };
    case "judge":
      return { ...base, kind, state: "", instructions: "", options: [], otherwise: "otherwise" };
    case "parallel":
      return { ...base, kind };
    case "for_each":
      return { ...base, kind, items: "", max_iterations: DEFAULT_MAX_ITERATIONS };
    case "while":
      return { ...base, kind, when: blankCondition(), max_iterations: DEFAULT_MAX_ITERATIONS };
    case "connector":
      return { ...base, kind, connector: null, operation: null, params: {} };
    case "wait":
      return { ...base, kind, until: { until: "release" } };
    case "emit":
      return { ...base, kind, signal: "" };
    case "notify":
      return { ...base, kind, template: "", mentions: [] };
    case "spawn":
      return { ...base, kind, statement_template: "", assignees: [], wait: true };
    case "end":
      return { ...base, kind };
    default:
      throw new Error(`not a step kind: ${kind}`);
  }
}

/** The id and the name *New workflow*'s one step carries: a start, by hand. */
export const START_STEP_ID = "start";

/**
 * A definition with nothing in it yet but its way in — what "New workflow"
 * opens: one start, by hand, so the canvas reads *Start · by hand* and a
 * first step drawn under it is already reached.
 */
export function blankWorkflow(name = t("workflow-goal-workflow-tab-untitled-workflow")) {
  return { name, description: "", inputs: [], steps: [blankStep("start", START_STEP_ID)], tags: [] };
}

/**
 * The branch labels a step's own kind names, in handle order: a `decide`'s
 * rules' then `otherwise`; a `switch`'s cases' then `otherwise`; the fixed
 * words of an `if` and of a loop. Empty for a kind whose flows are unlabelled.
 */
export function kindBranchesOf(step) {
  if (!step) return [];
  const out = [];
  const add = (b) => {
    if (b && !out.includes(b)) out.push(b);
  };
  switch (step.kind) {
    case "decide":
      for (const r of step.rules ?? []) add(r?.branch);
      add(step.otherwise);
      break;
    case "switch":
      for (const c of step.cases ?? []) add(c?.branch);
      add(step.otherwise);
      break;
    case "judge":
      for (const o of step.options ?? []) add(o?.branch);
      add(step.otherwise);
      break;
    case "if":
    case "for_each":
    case "while":
      for (const b of FIXED_BRANCHES[step.kind]) add(b);
      break;
    default:
      break;
  }
  return out;
}

/** The names of a step's boundary events that divert — the labels its divert flows carry, in declaration order. */
export function divertNamesOf(step) {
  const out = [];
  for (const b of step?.boundaries ?? []) {
    if (b?.act === "divert" && b.name && !out.includes(b.name)) out.push(b.name);
  }
  return out;
}

/**
 * Every label a step's flows may carry: its kind's branches, then the names
 * of its boundary events that divert. The rest of its flows are unlabelled.
 */
export function branchesOf(step) {
  const out = kindBranchesOf(step);
  for (const name of divertNamesOf(step)) if (!out.includes(name)) out.push(name);
  return out;
}

/** Whether a kind labels its flows — the kinds that fan out by branch. */
export function isBranching(step) {
  return (
    step?.kind === "decide" ||
    step?.kind === "switch" ||
    step?.kind === "judge" ||
    step?.kind === "if" ||
    step?.kind === "for_each" ||
    step?.kind === "while"
  );
}
