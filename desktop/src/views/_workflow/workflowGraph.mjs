/**
 * The workflow as a graph, and every edit the designer makes to one — with no
 * React and no canvas library in it.
 *
 * A `Workflow` on the wire is a list of steps, each carrying the flows that
 * leave it (`then`) and the boundary events heard while it is live. That list
 * *is* the graph; there is no second structure to keep in step. Every
 * function here takes a definition and returns a new one, so the designer's
 * undo history is a list of definitions and a save is whatever the list's
 * head says.
 *
 * What this module deliberately does **not** do is validate. Removing a step
 * strips the flows that pointed at it, because that is what the person
 * asked for; a reference that survives in a condition or a template becomes a
 * server problem the inspector shows, never a silent rewrite. The node's
 * `validate_workflow` is the one authority on what is wrong, and this module
 * exists to make the edits it will judge.
 */

import { FIXED_BRANCHES, blankStep, branchesOf, divertNamesOf, isBranching } from "./stepKinds.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * The graph, as nodes and edges the canvas draws. Every edge says whether it
 * is a **loop** — a flow back to an earlier step, the ones the run machine
 * treats as loops (`backEdges`) — so the canvas routes it around the side
 * and the layout keeps it out of the ranking. A flow labelled with one of
 * its source's diverting boundary events is a **boundary** edge: it leaves
 * from that boundary's chip, and it is taken only when the boundary fired.
 */
export function toGraph(wf) {
  const nodes = (wf.steps ?? []).map((s, order) => ({ id: s.id, step: s, order }));
  const edges = [];
  for (const s of wf.steps ?? []) {
    const diverts = isBranching(s) ? [] : divertNamesOf(s);
    for (const f of s.then ?? []) {
      edges.push({
        id: `${s.id}->${f.to}${f.branch ? `#${f.branch}` : ""}`,
        from: s.id,
        to: f.to,
        branch: f.branch ?? null,
        kind: f.branch && diverts.includes(f.branch) ? "boundary" : "then",
        loop: false,
      });
    }
    if (s.on_fail?.on_fail === "then" && s.on_fail.step) {
      edges.push({
        id: `${s.id}!>${s.on_fail.step}`,
        from: s.id,
        to: s.on_fail.step,
        branch: null,
        kind: "on_fail",
        loop: false,
      });
    }
  }
  const back = backEdges(
    nodes.map((n) => n.id),
    edges,
  );
  for (const e of edges) e.loop = back.has(e.id);
  return { nodes, edges };
}

/**
 * The loop edges: the set of edges a depth-first walk met while their target
 * was still on the stack. The walk starts where the core's
 * `Workflow::loop_edges` starts — the steps nothing flows into (every start
 * among them), else the first step — and then visits the rest in `steps`
 * order, so the edges the canvas draws as loops are exactly the ones the run
 * machine treats as loops.
 */
export function backEdges(order, edges) {
  const out = new Set();
  const state = new Map(); // id → "open" | "done"
  const outgoing = new Map();
  const targets = new Set();
  for (const e of edges) {
    if (!outgoing.has(e.from)) outgoing.set(e.from, []);
    outgoing.get(e.from).push(e);
    targets.add(e.to);
  }
  const visit = (id) => {
    state.set(id, "open");
    for (const e of outgoing.get(id) ?? []) {
      const s = state.get(e.to);
      if (s === "open") out.add(e.id);
      else if (!s) visit(e.to);
    }
    state.set(id, "done");
  };
  const roots = order.filter((id) => !targets.has(id));
  for (const id of roots.length > 0 ? roots : order.slice(0, 1)) if (!state.has(id)) visit(id);
  for (const id of order) if (!state.has(id)) visit(id);
  return out;
}

/** Every flow into one step: `{ from, branch }`. */
export function incoming(wf, id) {
  const out = [];
  for (const s of wf.steps ?? []) {
    for (const f of s.then ?? []) if (f.to === id) out.push({ from: s.id, branch: f.branch ?? null });
  }
  return out;
}

/**
 * The steps a flow leads from into `id`, however many flows away — the only
 * ones a condition, a template or a schema of `id` may read — in the
 * definition's own order. A step inside a loop that comes back to itself is
 * not its own upstream. Whether a step is *sure* to have run on every path
 * is the node's to judge; this is what the forms offer.
 */
export function upstreamOf(wf, id) {
  const from = new Map();
  for (const s of wf?.steps ?? []) {
    for (const f of s.then ?? []) {
      if (!from.has(f.to)) from.set(f.to, []);
      from.get(f.to).push(s.id);
    }
  }
  const seen = new Set();
  const queue = [...(from.get(id) ?? [])];
  for (let at = 0; at < queue.length; at += 1) {
    const cur = queue[at];
    if (seen.has(cur) || cur === id) continue;
    seen.add(cur);
    for (const before of from.get(cur) ?? []) queue.push(before);
  }
  return (wf?.steps ?? []).filter((s) => seen.has(s.id));
}

export { branchesOf, isBranching };

/**
 * An id no step has, from a kind. `agent`, `agent-2`, `agent-3`: readable in
 * a template (`{steps.agent-2.output}`) and inside the id grammar
 * (`[a-z][a-z0-9_-]{0,31}`).
 */
export function uniqueId(wf, kind) {
  const taken = new Set((wf.steps ?? []).map((s) => s.id));
  if (!taken.has(kind)) return kind;
  for (let n = 2; ; n++) {
    const id = `${kind}-${n}`;
    if (!taken.has(id)) return id;
  }
}

/**
 * Append a blank step of one kind — where the canvas says when it was dropped
 * there, placeless when it was clicked into being (the layout places it until
 * a person does). Returns the new definition and the id.
 */
export function addStep(wf, kind, id = uniqueId(wf, kind), position = null) {
  const step = position ? { ...blankStep(kind, id), position } : blankStep(kind, id);
  return { wf: { ...wf, steps: [...(wf.steps ?? []), step] }, id };
}

/** Put one step where a person dropped it. A step that is not there is left alone. */
export function setPosition(wf, id, position) {
  return setPositions(wf, new Map([[id, position]]));
}

/**
 * Put several steps where one gesture dropped them — a multi-selection
 * dragged together is one edit. Steps the map does not name keep their place.
 */
export function setPositions(wf, positions) {
  if (positions.size === 0) return wf;
  return { ...wf, steps: (wf.steps ?? []).map((s) => (positions.has(s.id) ? { ...s, position: positions.get(s.id) } : s)) };
}

/**
 * Remove a step and every flow into it — a divert's path that led there
 * included. An `on_fail: then` that pointed at it falls back to `fail` — the
 * default — because a remediation step that is gone leaves nothing to route
 * to. The step's own boundary events go with it. Conditions and templates
 * that name it are left as they are for the validator to report.
 */
export function removeStep(wf, id) {
  return {
    ...wf,
    steps: (wf.steps ?? [])
      .filter((s) => s.id !== id)
      .map((s) => ({
        ...s,
        then: (s.then ?? []).filter((f) => f.to !== id),
        on_fail: s.on_fail?.on_fail === "then" && s.on_fail.step === id ? { on_fail: "fail" } : s.on_fail,
      })),
  };
}

/**
 * Connect two steps, optionally on a branch — a gateway's or a loop's, or
 * a diverting boundary event's path, drawn from its chip.
 *
 * Refuses what the validator would refuse and the canvas can say at once: a
 * self flow, a duplicate, a flow into a start (a start begins a run), a flow
 * out of an end, a labelled flow out of a plain step that is no divert's, an
 * unlabelled one out of a gateway, a branch the gateway does not have, a
 * second flow on one branch. Anything else — a cycle, a second root — is
 * legal or is the validator's to judge.
 */
export function connect(wf, from, to, branch = null) {
  const src = (wf.steps ?? []).find((s) => s.id === from);
  const dst = (wf.steps ?? []).find((s) => s.id === to);
  if (!src || !dst) return { ok: false, reason: tr("workflow-workflow-graph-both-ends-must-steps") };
  if (from === to) return { ok: false, reason: tr("workflow-workflow-graph-step-cannot-flow-into-itself") };
  if (src.kind === "end") return { ok: false, reason: tr("workflow-workflow-graph-end-step-has-nothing-after") };
  if (dst.kind === "start") return { ok: false, reason: tr("workflow-workflow-graph-start-begins-run-nothing-flows-into") };
  if (isBranching(src)) {
    if (!branch) return { ok: false, reason: tr("workflow-workflow-graph-flow-out-step-carries-branch", { kind: src.kind }) };
    if (!branchesOf(src).includes(branch)) return { ok: false, reason: tr("workflow-workflow-graph-step-has-no-branch", { kind: src.kind, branch }) };
    if ((src.then ?? []).some((f) => f.branch === branch))
      return { ok: false, reason: tr("workflow-workflow-graph-branch-already-flows-somewhere", { branch }) };
  } else if (branch) {
    if (!divertNamesOf(src).includes(branch)) return { ok: false, reason: tr("workflow-workflow-graph-only-branching-step-decide-if-switch") };
    if ((src.then ?? []).some((f) => f.branch === branch))
      return { ok: false, reason: tr("workflow-workflow-graph-branch-already-flows-somewhere", { branch }) };
  }
  if ((src.then ?? []).some((f) => f.to === to && (f.branch ?? null) === branch))
    return { ok: false, reason: tr("workflow-workflow-graph-flow-already-exists") };
  const flow = branch ? { to, branch } : { to };
  return {
    ok: true,
    wf: {
      ...wf,
      steps: wf.steps.map((s) => (s.id === from ? { ...s, then: [...(s.then ?? []), flow] } : s)),
    },
  };
}

/** Remove one flow. A boundary's path cut this way leaves the boundary, which the validator then asks a path of. */
export function disconnect(wf, from, to, branch = null) {
  return {
    ...wf,
    steps: (wf.steps ?? []).map((s) =>
      s.id === from
        ? { ...s, then: (s.then ?? []).filter((f) => !(f.to === to && (f.branch ?? null) === branch)) }
        : s,
    ),
  };
}

/** What a new branch's name starts from, by kind: a switch's branches are its cases'. */
const BRANCH_ROOT = Object.freeze({ switch: "case" }); // for the machine

/**
 * A branch name the step has for nothing else — no rule, case or option,
 * not `otherwise`, no boundary that diverts: `branch-1`, `branch-2`… — a
 * switch's `case-1`… — the first that is free. What *Add a rule*, *Add a
 * case* and *Add an option* name the branch they add.
 */
export function freshBranch(step) {
  const taken = new Set([...branchesOf(step), step?.otherwise].filter(Boolean));
  const root = BRANCH_ROOT[step?.kind] ?? "branch"; // for the machine
  for (let n = 1; ; n++) {
    const name = `${root}-${n}`;
    if (!taken.has(name)) return name;
  }
}

/**
 * Rename a branch of a `decide` or a `switch` step everywhere the step knows
 * it: the rule or case that chooses it, `otherwise` when that is it, and
 * every flow out of the step labelled with it — so a rename never orphans a
 * flow. Refused when the new name is empty or another branch of the step
 * already has it. An `if` and a loop have fixed words and refuse a rename.
 */
export function relabelBranch(step, from, to) {
  const next = (to ?? "").trim();
  if (step?.kind === "if" || step?.kind === "for_each" || step?.kind === "while") {
    return { ok: false, reason: tr("workflow-graph-branches-fixed", { kind: step.kind, branches: FIXED_BRANCHES[step.kind].join(" and ") }) };
  }
  if (step?.kind !== "decide" && step?.kind !== "switch" && step?.kind !== "judge") {
    return { ok: false, reason: tr("workflow-workflow-graph-only-decide-switch-judge-step-has") };
  }
  if (next === from) return { ok: true, step };
  if (!next) return { ok: false, reason: tr("workflow-workflow-graph-branch-needs-name") };
  if (branchesOf(step).includes(next)) return { ok: false, reason: tr("workflow-workflow-graph-step-already-has-branch", { kind: step.kind, next }) };
  const renamed = {
    ...step,
    otherwise: step.otherwise === from ? next : step.otherwise,
    then: (step.then ?? []).map((f) => (f.branch === from ? { ...f, branch: next } : f)),
  };
  if (step.kind === "decide") renamed.rules = (step.rules ?? []).map((r) => (r.branch === from ? { ...r, branch: next } : r));
  else if (step.kind === "switch") renamed.cases = (step.cases ?? []).map((c) => (c.branch === from ? { ...c, branch: next } : c));
  else renamed.options = (step.options ?? []).map((o) => (o.branch === from ? { ...o, branch: next } : o));
  return { ok: true, step: renamed };
}

/**
 * Every `{<root>.<from>` placeholder in a string renamed to `{<root>.<to>`,
 * whole and dotted alike: `{inputs.old}` and `{inputs.old.x}` both move,
 * `{inputs.older}` does not. A doubled brace is the author's text and is
 * left alone: `{{inputs.old}}` is not a placeholder.
 */
function renameInTemplate(text, root, from, to) {
  if (typeof text !== "string") return text;
  // Doubled braces are read the way the engine reads them — as one literal
  // brace each — so they are set aside first and put back untouched.
  const OPEN = "";
  const CLOSE = "";
  const escaped = from.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return text
    .split("{{")
    .join(OPEN)
    .split("}}")
    .join(CLOSE)
    // A placeholder ends at a dot, a brace, or the brace a doubled pair set aside.
    .replace(new RegExp(`\\{${root}\\.${escaped}(?=[.}${CLOSE}])`, "g"), `{${root}.${to}`)
    .split(OPEN)
    .join("{{")
    .split(CLOSE)
    .join("}}");
}

/** A map's string values rewritten by `f` — a payload's, a parameter set's, exact fields'. */
function mapValues(m, f) {
  return m && typeof m === "object" ? Object.fromEntries(Object.entries(m).map(([k, v]) => [k, f(v)])) : m;
}

/** A filter's templates rewritten — a message's place and words, a signal's name and fields, a project's branch and glob, a topic's fields. */
function mapFilterTemplates(filter, f) {
  const next = { ...filter };
  if (typeof filter.in === "string") next.in = f(filter.in);
  if (typeof filter.contains === "string") next.contains = f(filter.contains);
  if (typeof filter.name === "string") next.name = f(filter.name);
  if (typeof filter.branch === "string") next.branch = f(filter.branch);
  if (typeof filter.glob === "string") next.glob = f(filter.glob);
  if (filter.fields) next.fields = mapValues(filter.fields, f);
  return next;
}

/** A catch's templates rewritten: a moment, and the filters. */
function mapWaitTemplates(until, f) {
  if (!until) return until;
  if (until.until === "time") return { ...until, at: f(until.at) };
  if (["signal", "message", "project", "platform"].includes(until.until)) return mapFilterTemplates(until, f);
  return until;
}

/** A start's event fields rewritten — templates over the inputs it listens with. Its mapping reads the event alone and is left as it is. */
function mapStartTemplates(on, f) {
  if (!on) return on;
  switch (on.event) {
    case "message":
    case "signal":
    case "project":
    case "platform":
      return mapFilterTemplates(on, f);
    case "connector":
      return { ...on, params: mapValues(on.params, f) };
    case "check":
      return { ...on, command: f(on.command) };
    default:
      return on;
  }
}

/** A boundary event's templates rewritten: its filter's, and its post's or its signal's. */
function mapBoundaryTemplates(b, f) {
  const next = { ...b, on: b.on && (b.on.event === "message" || b.on.event === "signal") ? mapFilterTemplates(b.on, f) : b.on };
  if (b.act === "notify") {
    next.template = f(b.template);
    if (typeof b.scope === "string") next.scope = f(b.scope);
  } else if (b.act === "emit") {
    next.signal = f(b.signal);
    if (b.payload) next.payload = mapValues(b.payload, f);
  }
  return next;
}

/** Every template string a step carries — its kind's and its boundary events' — rewritten by `f`. */
function mapTemplates(s, f) {
  const next = { ...s };
  switch (s.kind) {
    case "start":
      next.on = mapStartTemplates(s.on, f);
      break;
    case "agent":
      next.instructions = f(s.instructions);
      break;
    case "human":
    case "approval":
      next.prompt = f(s.prompt);
      break;
    case "check":
      if (s.check?.check === "command") next.check = { ...s.check, command: f(s.check.command) };
      break;
    case "wait":
      next.until = mapWaitTemplates(s.until, f);
      break;
    case "emit":
      next.signal = f(s.signal);
      if (s.payload) next.payload = mapValues(s.payload, f);
      break;
    case "notify":
      next.template = f(s.template);
      if (typeof s.scope === "string") next.scope = f(s.scope);
      break;
    case "spawn":
      next.statement_template = f(s.statement_template);
      break;
    case "switch":
      next.on = f(s.on);
      break;
    case "judge":
      next.state = f(s.state);
      next.instructions = f(s.instructions);
      break;
    case "for_each":
      next.items = f(s.items);
      break;
    case "connector":
      next.params = mapValues(s.params ?? {}, f);
      break;
    default:
      break;
  }
  if (Array.isArray(s.boundaries)) next.boundaries = s.boundaries.map((b) => mapBoundaryTemplates(b, f));
  return next;
}

/** A condition tree with `f` applied to every leaf. */
function mapConditions(c, f) {
  if (!c || typeof c !== "object") return c;
  if (c.condition === "not") return { ...c, of: mapConditions(c.of, f) };
  if (c.condition === "all" || c.condition === "any" || c.condition === "one") {
    return { ...c, of: (c.of ?? []).map((x) => mapConditions(x, f)) };
  }
  return f(c);
}

/** Every step's condition trees (`decide` rules, `if`, `while`), rewritten by `f` at the leaves. */
function mapStepConditions(s, f) {
  switch (s.kind) {
    case "decide":
      return { ...s, rules: (s.rules ?? []).map((r) => ({ ...r, when: mapConditions(r.when, f) })) };
    case "if":
    case "while":
      return { ...s, when: mapConditions(s.when, f) };
    default:
      return s;
  }
}

/**
 * Rename a step everywhere the definition knows it: `then`, `on_fail`,
 * `check.of`, every condition's `step`, and `{steps.<id>.…}` placeholders in
 * every template the kinds and the boundary events carry. The one edit that
 * has to be whole, because a half-renamed step is three validator problems
 * at once.
 */
export function renameStep(wf, from, to) {
  if (from === to) return wf;
  const rewrite = (s) => {
    const next = mapTemplates({ ...s, id: s.id === from ? to : s.id }, (t) => renameInTemplate(t, "steps", from, to));
    next.then = (s.then ?? []).map((f) => (f.to === from ? { ...f, to } : f));
    if (s.on_fail?.on_fail === "then" && s.on_fail.step === from) next.on_fail = { on_fail: "then", step: to };
    if (s.kind === "check" && s.check?.check === "schema" && s.check.of === from) next.check = { ...s.check, of: to };
    return mapStepConditions(next, (leaf) => (leaf && "step" in leaf && leaf.step === from ? { ...leaf, step: to } : leaf));
  };
  return { ...wf, steps: (wf.steps ?? []).map(rewrite) };
}

/** A `{ input: from }` reference renamed; anything else as it was. */
function renameRef(ref, from, to) {
  return ref && typeof ref === "object" && "input" in ref && ref.input === from ? { input: to } : ref;
}

/** A filter's or a cadence's references renamed: who wrote it or is mentioned, a project, an account, the seconds or the cron. */
function renameFilterRefs(filter, from, to) {
  const next = { ...filter };
  for (const key of ["from", "mentions", "project", "account", "every", "cron", "secs"]) {
    if (key in filter) next[key] = renameRef(filter[key], from, to);
  }
  return next;
}

/** A start's mapping with the input renamed: the mapping is keyed by the inputs it fills. */
function renameMappingKey(mapping, from, to) {
  if (!mapping || !(from in mapping)) return mapping;
  return Object.fromEntries(Object.entries(mapping).map(([k, v]) => [k === from ? to : k, v]));
}

/**
 * Rename an input everywhere the definition knows it: the declaration,
 * every `{inputs.<name>…}` placeholder in every template, every
 * `{ input: <name> }` reference a step carries (an assignee, a project, a
 * delay's seconds, a schedule's cron, a mention, a spawn's assignees, a
 * start's cadence, author or project, a boundary's clock, a post's voice),
 * every start's mapping that fills it, and every `decide` rule that reads
 * it. Whole for the same reason `renameStep` is: a half-renamed input is an
 * unknown input and a dead one at once.
 */
export function renameInput(wf, from, to) {
  if (from === to || !to) return wf;
  const rewrite = (s) => {
    const next = mapTemplates(s, (t) => renameInTemplate(t, "inputs", from, to));
    switch (s.kind) {
      case "start":
        next.on = renameFilterRefs(next.on ?? {}, from, to);
        if (s.inputs) next.inputs = renameMappingKey(s.inputs, from, to);
        break;
      case "agent":
        next.assignee = renameRef(s.assignee, from, to);
        next.project = renameRef(s.project, from, to);
        break;
      case "human":
        next.assignee = renameRef(s.assignee, from, to);
        break;
      case "wait":
        next.until = renameFilterRefs(next.until ?? {}, from, to);
        break;
      case "notify":
        next.mentions = (s.mentions ?? []).map((m) => renameRef(m, from, to));
        if (s.author) next.author = renameRef(s.author, from, to);
        break;
      case "spawn":
        next.assignees = (s.assignees ?? []).map((a) => renameRef(a, from, to));
        break;
      case "connector":
        next.account = renameRef(s.account, from, to);
        break;
      default:
        break;
    }
    if (Array.isArray(next.boundaries)) {
      next.boundaries = next.boundaries.map((b) => ({
        ...b,
        on: b.on ? renameFilterRefs(b.on, from, to) : b.on,
        ...(b.act === "notify" ? { mentions: (b.mentions ?? []).map((m) => renameRef(m, from, to)), ...(b.author ? { author: renameRef(b.author, from, to) } : {}) } : {}),
      }));
    }
    return mapStepConditions(next, (leaf) => (leaf && "input" in leaf && leaf.input === from ? { ...leaf, input: to } : leaf));
  };
  return {
    ...wf,
    inputs: (wf.inputs ?? []).map((i) => (i.name === from ? { ...i, name: to } : i)),
    steps: (wf.steps ?? []).map(rewrite),
  };
}

/**
 * The steps a failure may be routed to (`on_fail: then`): every other step
 * something may flow into — never the step itself, and never a `start`,
 * which begins a run and takes no flow and no fail route
 * (`start_has_incoming`).
 * @param {readonly object[] | null | undefined} steps
 * @param {string} id the step whose failure is routed
 */
export function failTargets(steps, id) {
  return (steps ?? []).filter((s) => s.id !== id && s.kind !== "start");
}

/**
 * The `on_fail` a choice in the step's form writes: `fail` — the default,
 * and what a word that is none reads as — `skip`, or `then` with the step
 * the failure is routed to: the route it already has while that is a step
 * a failure may reach, else the first such step. `null` when the choice is
 * `then` and there is nowhere to route — refused, so no blank reference is
 * ever written.
 * @param {readonly object[] | null | undefined} steps
 * @param {{id: string, on_fail?: {on_fail: string, step?: string}}} step
 * @param {string} word
 */
export function failChoice(steps, step, word) {
  if (word === "skip") return { on_fail: "skip" };
  if (word !== "then") return { on_fail: "fail" };
  const targets = failTargets(steps, step.id);
  const kept = step.on_fail?.on_fail === "then" ? step.on_fail.step : null;
  const to = targets.some((s) => s.id === kept) ? kept : (targets[0]?.id ?? null);
  return to === null ? null : { on_fail: "then", step: to };
}

/** One step's `on_fail`, set — what deleting an on-fail edge on the canvas does. */
export function setOnFail(wf, id, onFail) {
  if (!(wf.steps ?? []).some((s) => s.id === id)) return wf;
  return { ...wf, steps: wf.steps.map((s) => (s.id === id ? { ...s, on_fail: onFail } : s)) };
}

/** How far a duplicate lands from its source, right and down, so both stay in view. */
export const DUPLICATE_OFFSET = 24;

/**
 * A copy of one step under a fresh id, with no flows in or out — its
 * boundary events copied, their paths to draw again — one
 * [`DUPLICATE_OFFSET`] right and down from its source when the source has a
 * place, never on top of it.
 */
export function duplicateStep(wf, id) {
  const src = (wf.steps ?? []).find((s) => s.id === id);
  if (!src) return { wf, id: null };
  const next = uniqueId(wf, src.id.replace(/-\d+$/, ""));
  const copy = { ...structuredClone(src), id: next, name: tr("workflow-workflow-graph-copy", { src: src.name }), then: [] };
  if (src.position) copy.position = { x: src.position.x + DUPLICATE_OFFSET, y: src.position.y + DUPLICATE_OFFSET };
  return { wf: { ...wf, steps: [...wf.steps, copy] }, id: next };
}

/** Replace one step wholesale — what a form's save does. */
export function replaceStep(wf, id, step) {
  return { ...wf, steps: (wf.steps ?? []).map((s) => (s.id === id ? step : s)) };
}

/**
 * The one gate every canvas edit passes: nothing when the canvas is read-only,
 * and — in an amendment, where `editable` names the steps that have not
 * started — only those steps. `editable` null means every step. A connection
 * is judged by its source, a deletion by the step, a drop by nothing but the
 * gate itself (`id` null).
 */
export function mayEdit(gate, id = null) {
  if (gate?.readOnly) return false;
  if (!gate?.editable) return true;
  return id === null ? true : gate.editable.has(id);
}

