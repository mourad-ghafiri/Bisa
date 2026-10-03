import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  HOLDERS,
  HOLDER_FILTER_ALL,
  HOLDER_LABEL,
  HOLDER_FILTER_LABEL,
  HOLDER_TONE,
  KIND_LABEL,
  chipsOf,
  currentStepOf,
  finishedTone,
  matchesFilters,
  parseFilters,
  serializeFilters,
  sortByActivity,
  workflowFacets, failureWords } from "./goalStripModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../crates/bisa-core/src");

/** CamelCase variants of a Rust enum, read from the source (recipe 7). */
function variantsOf(file, name) {
  const src = readFileSync(join(CORE, file), "utf8");
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in ${file}`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) =>
    m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase(),
  );
}

const ROLES = readFileSync(join(HERE, "../../theme/tokens.css"), "utf8");
const isRole = (t) => t === "accent" || ROLES.includes(`--color-${t}`) || ROLES.includes(`--${t}`);

test("every core holder has a label and a theme-role tone, in the core's order", () => {
  const holders = variantsOf("goal.rs", "Holder");
  assert.deepEqual([...HOLDERS], holders);
  for (const h of holders) {
    assert.ok(HOLDER_LABEL[h], `label for ${h}`);
    assert.ok(HOLDER_FILTER_LABEL[h] && HOLDER_FILTER_LABEL[h][0] === HOLDER_FILTER_LABEL[h][0].toUpperCase(), `a sentence-case filter option for ${h}`);
    assert.ok(HOLDER_TONE[h], `tone for ${h}`);
    assert.ok(isRole(HOLDER_TONE[h]), `${HOLDER_TONE[h]} is a theme role`);
  }
});

test("every step kind has a label, and a chip renders for each", () => {
  const kinds = variantsOf("workflow.rs", "StepKind");
  for (const k of kinds) assert.ok(KIND_LABEL[k], `label for ${k}`);
  const strip = {
    steps: kinds.map((k, i) => ({ id: `s${i}`, name: k, kind: k, state: { state: "pending" } })),
    current: [],
  };
  assert.equal(chipsOf(strip).length, kinds.length);
  // Each is a message of the catalog: a kind's word is never written in the model.
  assert.deepEqual(
    ["human", "approval", "check", "decide", "judge", "if", "switch", "while", "wait", "notify", "end"].map((k) => KIND_LABEL[k]),
    ["question", "approval", "check", "decision", "judgement", "condition", "switch", "loop", "wait", "notification", "end"],
  );
  const model = readFileSync(join(HERE, "goalStripModel.mjs"), "utf8");
  const table = model.slice(model.indexOf("export const KIND_LABEL"), model.indexOf("}", model.indexOf("export const KIND_LABEL")));
  assert.deepEqual(table.split("\n").slice(1).filter((line) => line.trim() && !/: t\("goals-goal-strip-[a-z-]+"\),$/.test(line.trim())), [], "every row reads the catalog");
});

test("exactly running and waiting count as current, and every state chips", () => {
  const states = variantsOf("run.rs", "StepState");
  assert.ok(states.includes("running") && states.includes("waiting"));
  const steps = states.map((st, i) => ({
    id: `s${i}`,
    name: st,
    kind: "agent",
    state: { state: st },
  }));
  const current = steps
    .filter((s) => s.state.state === "running" || s.state.state === "waiting")
    .map((s) => s.id);
  const chips = chipsOf({ steps, current });
  assert.equal(chips.length, states.length);
  for (const c of chips) {
    assert.equal(c.current, c.state.state === "running" || c.state.state === "waiting");
    assert.ok(c.label.includes(c.state.state));
  }
});

test("the current step is the first live one, with the surplus counted", () => {
  const strip = {
    steps: [
      { id: "a", name: "A", kind: "agent", state: { state: "done" } },
      { id: "b", name: "B", kind: "human", state: { state: "waiting" } },
      { id: "c", name: "C", kind: "check", state: { state: "running" } },
    ],
    current: ["b", "c"],
  };
  const cur = currentStepOf(strip);
  assert.equal(cur.id, "b");
  assert.equal(cur.more, 1);
  assert.equal(currentStepOf({ steps: [], current: [] }), null);
});

test("filters compose, and an unknown holder word hides nothing", () => {
  const row = {
    id: "01A",
    title: "Dark mode",
    statement: "ship the toggle",
    holder: "you",
    strip: { workflow_name: "Software feature" },
    last_activity_at: 5,
  };
  assert.ok(matchesFilters(row, {}));
  assert.ok(matchesFilters(row, { holder: "you", workflow: "Software feature", q: "toggle" }));
  assert.ok(!matchesFilters(row, { holder: "agents" }));
  assert.ok(!matchesFilters(row, { workflow: "Bug fix" }));
  assert.ok(!matchesFilters(row, { q: "checkout" }));
  assert.ok(matchesFilters(row, { holder: "flight" }), "an unknown word keeps the row");
});

test("newest movement first, ties by id newest first, input untouched", () => {
  const rows = [
    { id: "01A", holder: "you", last_activity_at: 1 },
    { id: "01C", holder: "you", last_activity_at: 9 },
    { id: "01B", holder: "you", last_activity_at: 1 },
  ];
  const sorted = sortByActivity(rows);
  assert.deepEqual(sorted.map((r) => r.id), ["01C", "01B", "01A"]);
  assert.deepEqual(rows.map((r) => r.id), ["01A", "01C", "01B"]);
});

test("workflow facets count by name, alphabetically", () => {
  const rows = [
    { id: "1", holder: "you", last_activity_at: 0, strip: { workflow_name: "B" } },
    { id: "2", holder: "you", last_activity_at: 0, strip: { workflow_name: "A" } },
    { id: "3", holder: "you", last_activity_at: 0, strip: { workflow_name: "B" } },
    { id: "4", holder: "design", last_activity_at: 0, strip: {} },
  ];
  assert.deepEqual(workflowFacets(rows), [
    { name: "A", count: 1 },
    { name: "B", count: 2 },
  ]);
});

test("filters round trip through the URL", () => {
  const f = { holder: "you", workflow: "Bug fix", q: "login", archived: true };
  const params = new URLSearchParams(serializeFilters(f));
  assert.deepEqual(parseFilters(params), f);
  assert.equal(new URLSearchParams(serializeFilters(f)).get("archived"), "1");
  assert.equal(parseFilters(new URLSearchParams("archived=0")).archived, undefined, "only 1 means on");
  assert.deepEqual(serializeFilters({}), {});
  assert.deepEqual(parseFilters(new URLSearchParams()), {
    holder: undefined,
    workflow: undefined,
    q: undefined,
    archived: undefined,
  });
});

test("a failed outcome turns the finished tone to danger", () => {
  assert.equal(finishedTone({ outcome: "failed" }), "danger");
  assert.equal(finishedTone({ outcome: "done" }), "ok");
  assert.equal(finishedTone(undefined), "ok");
});

test("a failed strip says where and why on the row, and nothing while nothing failed", () => {
  assert.equal(
    failureWords({ outcome: "failed", failure: { step: "review", name: "Review", error: "{steps.hook.output.issue} has no value in this run: step `hook` yielded `status` and no `issue`" } }),
    "Failed at Review — {steps.hook.output.issue} has no value in this run: step `hook` yielded `status` and no `issue`",
  );
  assert.equal(failureWords({ outcome: "failed", failure: { step: "review", name: "", error: null } }), "Failed at review", "the id when the step has no name, no dash without an error");
  assert.equal(failureWords({ outcome: "done" }), null);
  assert.equal(failureWords(undefined), null);
});

test("All is a real segment id, not a holder, and reads as no filter", () => {
  assert.ok(HOLDER_FILTER_ALL.length > 0, "the kit drops an empty id");
  assert.ok(!HOLDERS.includes(HOLDER_FILTER_ALL));
  assert.equal(parseFilters(new URLSearchParams("holder=all")).holder, undefined);
  assert.equal(parseFilters(new URLSearchParams("holder=you")).holder, "you");
  const row = { id: "g", holder: "agents", strip: { workflow_name: "wf" }, title: "t", statement: "s", last_activity_at: 1 };
  assert.ok(matchesFilters(row, { holder: HOLDER_FILTER_ALL }), "All keeps every row");
  assert.deepEqual(serializeFilters({ holder: undefined }), {}, "no filter writes no key");
});
