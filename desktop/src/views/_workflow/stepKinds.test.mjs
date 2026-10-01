/**
 * The designer's vocabulary against the core's.
 *
 * Every closed set here is read out of the Rust source rather than restated:
 * a step kind added in `workflow.rs` without a palette entry, a condition
 * without a rule editor, a problem kind without a sentence, a start event,
 * a catch or a boundary event without a word — each fails here before it
 * fails on a canvas, which is the only ordering that cannot ship a hole.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  BOUNDARY_ACTS,
  BOUNDARY_EVENTS,
  CHECKS,
  COMBINATORS,
  CONDITIONS,
  DECIDE_PICKS,
  DEFAULT_MAX_ITERATIONS,
  DEFAULT_MAX_VISITS,
  END_FINISHES,
  FAMILIES,
  FAMILY_LABEL,
  FIRE_ON,
  FIXED_BRANCHES,
  JOINS,
  MAX_CONDITION_DEPTH,
  MESSAGE_FROM,
  ON_FAILS,
  OVERLAPS,
  PROBLEM_KIND_LABEL,
  PROJECT_CHANGES,
  RUN_ENDS,
  START_EVENTS,
  START_STEP_ID,
  STEP_KINDS,
  STEP_MIME,
  WAITS,
  BRANCH_HANDLE_PREFIX,
  blankCondition,
  blankStep,
  blankWait,
  blankWorkflow,
  branchHandle,
  branchOfHandle,
  branchesOf,
  divertNamesOf,
  familyOf,
  freshOptionId,
  isBranching,
  kindBranchesOf,
  kindLabel,
  mayCarryBoundaries,
} from "./stepKinds.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../crates/bisa-core/src");

const snake = (s) => s.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
const source = (file) => readFileSync(join(CORE, file), "utf8");

/**
 * The variants of one `pub enum` in a Rust source, as their serde
 * `snake_case` names — the tag values the wire carries.
 */
function variants(file, name) {
  const src = source(file);
  const m = src.match(new RegExp(`pub enum ${name}(?:<[^>]*>)? \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(m, `${name} is declared in ${file}`);
  const out = [];
  for (const line of m[1].split("\n")) {
    const v = line.match(/^\s{4}([A-Z][A-Za-z0-9]*)\s*(\{|\(|,)/);
    if (v) out.push(snake(v[1]));
  }
  assert.ok(out.length > 0, `${name} has variants`);
  return out;
}

/** `StepKind::NAMES`, in the core's palette order. */
function kindNames() {
  const src = source("workflow.rs");
  const impl = src.indexOf("impl StepKind {");
  assert.ok(impl >= 0, "StepKind has its impl");
  const at = src.indexOf("pub const NAMES", impl);
  return [...src.slice(at, src.indexOf("];", at)).matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
}

/** `StepKind::family`, kind by kind, read off its match arms. */
function families() {
  const src = source("workflow.rs");
  const at = src.indexOf("pub fn family(&self) -> Family {");
  assert.ok(at >= 0, "the core names each kind's family");
  const body = src.slice(at, src.indexOf("\n    }\n", at));
  const out = {};
  let pending = [];
  for (const m of body.matchAll(/StepKind::([A-Z][A-Za-z]*)|=> Family::([A-Z][a-z]+)/g)) {
    if (m[1]) pending.push(snake(m[1]));
    else {
      for (const k of pending) out[k] = m[2].toLowerCase();
      pending = [];
    }
  }
  return out;
}

/** The kinds `may_carry_boundaries` answers `true` for outright, and the one it answers by its `wait`. */
function boundaryCarriers() {
  const src = source("boundary.rs");
  const at = src.indexOf("pub fn may_carry_boundaries");
  assert.ok(at >= 0, "the core says which kinds carry boundaries");
  const body = src.slice(at, src.indexOf("\n}\n", at));
  const always = body.slice(0, body.indexOf("=> true")).match(/StepKind::([A-Z][A-Za-z]*)/g).map((s) => snake(s.slice("StepKind::".length)));
  const byWait = [...body.matchAll(/StepKind::([A-Z][A-Za-z]*) \{ wait, \.\. \} => \*wait/g)].map((m) => snake(m[1]));
  return { always, byWait };
}

test("the palette is exactly the core's step kinds, in the core's order, each in its family", () => {
  assert.deepEqual(STEP_KINDS.map((k) => k.kind), kindNames());
  assert.deepEqual([...STEP_KINDS.map((k) => k.kind)].sort(), variants("workflow.rs", "StepKind").sort());
  const core = families();
  for (const k of STEP_KINDS) {
    assert.equal(k.family, core[k.kind], `${k.kind} is a ${core[k.kind]}`);
    assert.equal(familyOf(k.kind), k.family);
    assert.equal(kindLabel(k.kind), k.label);
    assert.ok(k.explain.endsWith("."), `${k.kind} explains itself in a sentence`);
  }
  assert.deepEqual([...FAMILIES], variants("workflow.rs", "Family"), "the four families, in the palette's order");
  for (const f of FAMILIES) assert.ok(FAMILY_LABEL[f], `${f} has a heading`);
  assert.equal(familyOf("teleport"), "task", "a kind this build does not know is drawn as a task");
  assert.equal(kindLabel("teleport"), "teleport");
});

test("the rule editor knows every condition the core evaluates, and none reads the event", () => {
  assert.deepEqual(
    [...CONDITIONS.map((c) => c.condition)].sort(),
    variants("workflow.rs", "Condition").sort(),
  );
  for (const c of CONDITIONS) assert.doesNotMatch(c.condition, /payload/, "a start maps the event onto inputs; a rule reads those");
});

test("catches, checks, joins, on-fail, picks and finishes are the core's closed sets", () => {
  assert.deepEqual([...WAITS.map((w) => w.until)].sort(), variants("workflow.rs", "WaitFor").sort());
  assert.deepEqual([...CHECKS.map((c) => c.check)].sort(), variants("workflow.rs", "CheckKind").sort());
  assert.deepEqual([...JOINS.map((j) => j.join)].sort(), variants("workflow.rs", "Join").sort());
  assert.deepEqual([...ON_FAILS.map((o) => o.on_fail)].sort(), variants("workflow.rs", "OnFail").sort());
  assert.deepEqual(DECIDE_PICKS.map((p) => p.pick), variants("workflow.rs", "Pick"), "the first rule, then every rule");
  assert.deepEqual(END_FINISHES.map((f) => f.finish), variants("workflow.rs", "Finish"), "this path, the run done, the run failed");
});

test("start events, the guard's overlap and a check start's results are the core's", () => {
  assert.deepEqual(START_EVENTS.map((e) => e.event), variants("start.rs", "StartOn"), "in the designer's order");
  assert.deepEqual(OVERLAPS.map((o) => o.overlap), variants("start.rs", "Overlap"));
  assert.deepEqual(FIRE_ON.map((f) => f.fire_on), variants("start.rs", "FireOn"));
  assert.equal(START_EVENTS[0].event, "manual", "by hand comes first");
});

test("boundary events and their acts are the core's, and only the kinds whose work can be stopped carry them", () => {
  assert.deepEqual(BOUNDARY_EVENTS.map((e) => e.event), variants("boundary.rs", "BoundaryOn"));
  assert.deepEqual(BOUNDARY_ACTS.map((a) => a.act), variants("boundary.rs", "BoundaryAct"));
  const { always, byWait } = boundaryCarriers();
  assert.deepEqual([...always].sort(), ["agent", "approval", "human", "wait"]);
  assert.deepEqual(byWait, ["spawn"]);
  for (const { kind } of STEP_KINDS) {
    const carries = mayCarryBoundaries(blankStep(kind, "s"));
    assert.equal(carries, always.includes(kind) || byWait.includes(kind), `${kind}: ${carries}`);
  }
  assert.equal(mayCarryBoundaries({ ...blankStep("spawn", "s"), wait: false }), false, "a spawn that does not wait is done the moment its child exists");
  assert.equal(mayCarryBoundaries(null), false);
});

test("a message's author, a project's change and a run's end are the filters' words", () => {
  assert.deepEqual(MESSAGE_FROM.map((m) => m.from), variants("listen.rs", "MessageFrom"));
  assert.deepEqual(PROJECT_CHANGES.map((c) => c.change), variants("listen.rs", "ProjectChange"));
  assert.deepEqual(RUN_ENDS.map((r) => r.outcome), variants("listen.rs", "RunEnd"));
});

test("every closed set's words are words, not tags", () => {
  const sets = [START_EVENTS, WAITS, BOUNDARY_EVENTS, BOUNDARY_ACTS, END_FINISHES, DECIDE_PICKS, OVERLAPS, MESSAGE_FROM, PROJECT_CHANGES, RUN_ENDS, FIRE_ON];
  for (const set of sets) {
    for (const item of set) {
      assert.ok(item.label.length > 1, `${JSON.stringify(item)} has a label`);
      assert.doesNotMatch(item.label, /_|-[a-z]+-/, `${item.label} is words`);
    }
  }
});

test("every problem kind has a sentence a designer can act on", () => {
  const kinds = variants("workflow.rs", "ProblemKind");
  assert.deepEqual(Object.keys(PROBLEM_KIND_LABEL).sort(), kinds.sort());
  for (const [kind, label] of Object.entries(PROBLEM_KIND_LABEL)) {
    assert.ok(label.length > 8, `${kind} has a label`);
    assert.doesNotMatch(label, /_/, `${kind}'s label is words, not a tag`);
  }
  for (const kind of ["start_has_incoming", "many_manual_starts", "manual_start_configured", "start_placeholder", "boundary_on_instant_step", "reminder_interrupts", "bad_timer", "bad_signal_name", "bad_poll", "unknown_topic", "spawn_needs_manual_entry"]) {
    assert.ok(PROBLEM_KIND_LABEL[kind], `the events' ${kind} has words`);
  }
  assert.ok(kinds.includes("unsupported_effort"), "an effort pin no harness can set is a problem by name");
  assert.equal(PROBLEM_KIND_LABEL.unsupported_effort, "pins an effort none of its harnesses can set");
});

test("a blank step carries the core's defaults and the kind's required fields", () => {
  for (const { kind } of STEP_KINDS) {
    const s = blankStep(kind, "s1");
    assert.equal(s.id, "s1");
    assert.equal(s.kind, kind);
    assert.deepEqual(s.then, []);
    assert.equal(s.join, "all");
    assert.deepEqual(s.on_fail, { on_fail: "fail" });
    assert.equal(s.retries, 0);
    assert.equal(s.max_visits, DEFAULT_MAX_VISITS);
  }
  assert.equal(blankStep("agent", "a").tier_ceiling, "write");
  assert.equal(blankStep("wait", "w").until.until, "release");
  assert.equal(blankStep("check", "c").check.check, "command");
  assert.equal(blankStep("decide", "d").otherwise, "otherwise");
  assert.equal(blankStep("decide", "d").pick, undefined, "the first rule that holds, unwritten");
  assert.deepEqual(blankStep("if", "i").when, { condition: "all", of: [] });
  assert.deepEqual(blankStep("switch", "s").cases, []);
  assert.equal(blankStep("switch", "s").otherwise, "otherwise");
  assert.deepEqual(blankStep("judge", "j").options, []);
  assert.equal(blankStep("judge", "j").otherwise, "otherwise");
  assert.equal(blankStep("judge", "j").state, "");
  assert.equal(blankStep("judge", "j").instructions, "");
  assert.equal(blankStep("for_each", "f").max_iterations, DEFAULT_MAX_ITERATIONS);
  assert.equal(blankStep("while", "w").max_iterations, DEFAULT_MAX_ITERATIONS);
  assert.deepEqual(blankStep("while", "w").when, { condition: "all", of: [] });
  assert.deepEqual(blankStep("connector", "c").params, {});
  assert.throws(() => blankStep("nonsense", "x"), /not a step kind/);
});

test("the new kinds are born minimal: a start by hand with no mapping or guard, a parallel with nothing, an emit naming no signal yet, an end that ends its path", () => {
  const start = blankStep("start", "s");
  assert.deepEqual(start.on, { event: "manual" });
  assert.equal(start.inputs, undefined, "a start by hand maps nothing");
  assert.equal(start.guard, undefined, "and guards nothing");
  const parallel = blankStep("parallel", "p");
  assert.deepEqual(Object.keys(parallel).sort(), ["id", "join", "kind", "max_visits", "name", "on_fail", "retries", "then"]);
  assert.equal(blankStep("emit", "e").signal, "");
  const end = blankStep("end", "e");
  assert.equal(end.finish, undefined, "ends its path, unwritten");
  assert.equal(end.outcome, undefined, "the retired field is not written");
});

test("a fresh catch of every kind carries what the kind needs, and a reference not chosen is null", () => {
  for (const { until } of WAITS) {
    const w = blankWait(until);
    assert.equal(w.until, until);
  }
  assert.deepEqual(blankWait("delay"), { until: "delay", secs: 3600 });
  assert.deepEqual(blankWait("signal"), { until: "signal", name: "", fields: {} });
  assert.deepEqual(blankWait("project"), { until: "project", project: null, change: "commit" });
  assert.deepEqual(blankWait("platform"), { until: "platform", topic: "" });
  assert.deepEqual(blankWait("time"), { until: "time", at: "" });
  assert.throws(() => blankWait("forever"), /not a catch/);
});

/** Every string-valued field of a step that names another thing by id. */
const REFERENCE_FIELDS = new Set(["connector", "operation", "step", "of", "input", "workflow", "project", "assignee"]);

function referencesIn(value, path = []) {
  const out = [];
  if (Array.isArray(value)) {
    value.forEach((v, i) => out.push(...referencesIn(v, [...path, String(i)])));
  } else if (value && typeof value === "object") {
    for (const [k, v] of Object.entries(value)) {
      if (REFERENCE_FIELDS.has(k) && typeof v === "string") out.push([[...path, k].join("."), v]);
      out.push(...referencesIn(v, [...path, k]));
    }
  }
  return out;
}

test("no blank step or catch is born with a blank reference: a reference is a real id or absent", () => {
  // The node parses an id at the wire, so `""` is a body it refuses — a
  // refusal the designer would retry forever — while `null` is a choice not
  // made yet, which the validator lists as a problem.
  for (const { kind } of STEP_KINDS) {
    const s = blankStep(kind, "s1");
    for (const [where, v] of referencesIn(s)) assert.notEqual(v, "", `${kind}.${where} is not a blank reference`);
  }
  for (const { until } of WAITS) {
    for (const [where, v] of referencesIn(blankWait(until))) assert.notEqual(v, "", `${until}.${where} is not a blank reference`);
  }
  const c = blankStep("connector", "c");
  assert.equal(c.connector, null);
  assert.equal(c.operation, null);
  assert.deepEqual(blankCondition(), { condition: "all", of: [] }, "a blank condition names nothing");
});

test("a fresh option id is one no option has, however the existing ones are numbered", () => {
  assert.equal(freshOptionId([]), "option-1");
  assert.equal(freshOptionId([{ id: "option-1" }]), "option-2");
  assert.equal(freshOptionId([{ id: "option-2" }]), "option-1", "the gap is filled");
  assert.equal(freshOptionId([{ id: "option-1" }, { id: "sqlite" }, { id: "option-2" }]), "option-3");
  // The collision `option-${length + 1}` used to make: two removed, one added.
  assert.equal(freshOptionId([{ id: "option-3" }]), "option-1");
});

test("a branch handle is prefixed, so a branch may be named like a shared handle", () => {
  assert.equal(branchHandle("yes"), `${BRANCH_HANDLE_PREFIX}yes`);
  assert.equal(branchOfHandle(branchHandle("out")), "out", "a branch called `out` is a branch, not the out handle");
  assert.equal(branchOfHandle(branchHandle("fail")), "fail");
  assert.equal(branchOfHandle("out"), null);
  assert.equal(branchOfHandle("fail"), null);
  assert.equal(branchOfHandle("loop-out"), null);
  assert.equal(branchOfHandle(null), null);
  assert.equal(branchOfHandle(undefined), null);
  for (const b of ["out", "fail", "loop-out", "loop-in", "in"]) assert.notEqual(branchHandle(b), b);
});

test("max iterations and the nesting bound default to the core's", () => {
  const src = source("workflow.rs");
  const m = src.match(/pub const DEFAULT_MAX_ITERATIONS: u16 = (\d+);/);
  assert.ok(m, "the core names its loop bound");
  assert.equal(DEFAULT_MAX_ITERATIONS, Number(m[1]));
  const d = src.match(/pub const MAX_DEPTH: usize = (\d+);/);
  assert.ok(d, "the core bounds condition nesting");
  assert.equal(MAX_CONDITION_DEPTH, Number(d[1]));
});

test("the combinators are exactly the conditions that hold other conditions", () => {
  const src = source("workflow.rs");
  const body = src.match(/pub enum Condition \{([\s\S]*?)\n\}/)[1];
  // A variant whose first field is `of`, however the formatter laid it out:
  // on one line (`All { of: Vec<Condition> },`) or one field to a line.
  const holding = [...body.matchAll(/^ {4}([A-Z][A-Za-z]*) \{\s*of: /gm)].map((m) => snake(m[1]));
  assert.ok(holding.length > 0, "the core's combinators were read");
  assert.deepEqual([...COMBINATORS].sort(), holding.sort());
  for (const c of COMBINATORS) assert.ok(CONDITIONS.some((x) => x.condition === c), `${c} is in the editor`);
});

test("the fixed branch words are the core's", () => {
  const src = source("workflow.rs");
  const words = Object.fromEntries(
    [...src.matchAll(/pub const ([A-Z]+): &str = "([a-z]+)";/g)].map((m) => [m[1], m[2]]),
  );
  assert.deepEqual(FIXED_BRANCHES.if, [words.YES, words.NO]);
  assert.deepEqual(FIXED_BRANCHES.for_each, [words.EACH, words.DONE]);
  assert.deepEqual(FIXED_BRANCHES.while, [words.LOOP, words.DONE]);
});

test("max visits defaults to the core's three", () => {
  const src = source("workflow.rs");
  const m = src.match(/pub const DEFAULT_MAX_VISITS: u8 = (\d+);/);
  assert.ok(m, "the core names its default");
  assert.equal(DEFAULT_MAX_VISITS, Number(m[1]));
});

test("a blank workflow is a valid request shape with one way in: a start, by hand", () => {
  const w = blankWorkflow();
  assert.equal(typeof w.name, "string");
  assert.equal(w.steps.length, 1);
  assert.equal(w.steps[0].id, START_STEP_ID);
  assert.equal(w.steps[0].kind, "start");
  assert.deepEqual(w.steps[0].on, { event: "manual" });
  assert.deepEqual(w.inputs, []);
  assert.deepEqual(w.tags, []);
  assert.equal(blankWorkflow("Ship it").name, "Ship it");
  assert.notEqual(blankWorkflow().steps[0], blankWorkflow().steps[0], "each workflow its own start");
});

test("a decide step's branches are its rules' labels then otherwise, deduplicated", () => {
  const step = {
    kind: "decide",
    rules: [
      { when: { condition: "outcome", step: "t", passed: true }, branch: "pass" },
      { when: { condition: "outcome", step: "t", passed: false }, branch: "pass" },
      { when: { condition: "between", from_hour: 1, to_hour: 2 }, branch: "night" },
    ],
    otherwise: "fail",
  };
  assert.deepEqual(branchesOf(step), ["pass", "night", "fail"]);
  assert.deepEqual(branchesOf({ ...step, pick: "every" }), ["pass", "night", "fail"], "every rule that holds chooses from the same branches");
  assert.deepEqual(branchesOf({ kind: "agent" }), []);
  assert.deepEqual(branchesOf(null), []);
});

test("every branching kind names its branches and the plain kinds name none", () => {
  assert.deepEqual(branchesOf({ kind: "if", when: { condition: "between", from_hour: 1, to_hour: 2 } }), ["yes", "no"]);
  assert.deepEqual(branchesOf({ kind: "for_each", items: "{inputs.list}" }), ["each", "done"]);
  assert.deepEqual(branchesOf({ kind: "while", when: { condition: "between", from_hour: 1, to_hour: 2 } }), ["loop", "done"]);
  assert.deepEqual(
    branchesOf({ kind: "switch", on: "{inputs.env}", cases: [{ value: "prod", branch: "live" }, { value: "qa", branch: "test" }, { value: "x", branch: "live" }], otherwise: "rest" }),
    ["live", "test", "rest"],
  );
  assert.deepEqual(
    branchesOf({
      kind: "judge",
      state: "{steps.triage.output}",
      instructions: "Is this urgent?",
      options: [{ branch: "urgent", meaning: "needs a person today" }, { branch: "routine", meaning: "can wait" }],
      otherwise: "unsure",
    }),
    ["urgent", "routine", "unsure"],
  );
  for (const kind of ["decide", "if", "switch", "judge", "for_each", "while"]) assert.ok(isBranching({ kind }), `${kind} branches`);
  for (const kind of ["start", "agent", "human", "approval", "check", "connector", "wait", "emit", "parallel", "notify", "spawn", "end"]) assert.ok(!isBranching({ kind }), `${kind} does not`);
});

test("a step's labels are its kind's branches and its diverting boundaries' names — an act labels nothing", () => {
  const review = {
    kind: "approval",
    prompt: "Ship it?",
    boundaries: [
      { name: "late", on: { event: "after", secs: 172800 }, act: "divert" },
      { name: "nudge", on: { event: "every", secs: 86400, max: 5 }, act: "notify", template: "Still waiting." },
      { name: "cancelled", on: { event: "message", contains: "cancel" }, act: "divert" },
      { name: "heads-up", on: { event: "signal", name: "deploy.started" }, act: "emit", signal: "review.waiting" },
    ],
  };
  assert.deepEqual(kindBranchesOf(review), [], "an approval's own flows are unlabelled");
  assert.deepEqual(divertNamesOf(review), ["late", "cancelled"], "in declaration order");
  assert.deepEqual(branchesOf(review), ["late", "cancelled"]);
  assert.deepEqual(divertNamesOf({ kind: "agent" }), [], "no boundaries, no labels");
  assert.deepEqual(divertNamesOf(null), []);
  assert.ok(!isBranching(review), "a divert is not a branching kind: its normal flows stay unlabelled");
});

test("the drag MIME type is ours and not a text type a drop zone would misread", () => {
  assert.match(STEP_MIME, /^application\/x-bisa-/);
});
