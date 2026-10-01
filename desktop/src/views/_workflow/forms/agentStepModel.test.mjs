/**
 * An agent step's effort pin: the harnesses it may run on, and the sentence
 * under the pin. Run with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/views/_workflow/forms/agentStepModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { answersFor, harnessText, harnessesFrom, stepEffortHint, stepEfforts, stepHarnesses } from "./agentStepModel.mjs";

const CLAUDE_CODE = { harness: "claude-code", efforts: ["low", "medium", "high", "max"], models: [{ id: "claude-opus-5-5[1m]", efforts: ["low", "medium", "high", "xhigh", "max"] }, { id: "claude-haiku-4-5", efforts: [] }] };
const CODEX = { harness: "codex", efforts: ["low", "medium", "high"], models: [] };
const GOOSE = { harness: "goose", efforts: [], models: [] };

const AGENTS = [
  { id: "developer", harness: "claude-code" },
  { id: "reviewer", harness: "codex" },
];

test("a step runs on the harnesses it names, else on its one agent's; nobody can say for an input, a team or no assignee", () => {
  assert.deepEqual(stepHarnesses({ harness: ["codex", "claude-code"], assignee: { agent: "developer" } }, AGENTS), ["codex", "claude-code"], "the step's own order wins");
  assert.deepEqual(stepHarnesses({ harness: ["codex", "codex", ""] }, AGENTS), ["codex"], "each once, a blank left out");
  assert.deepEqual(stepHarnesses({ harness: [], assignee: { agent: "developer" } }, AGENTS), ["claude-code"]);
  assert.deepEqual(stepHarnesses({ assignee: { agent: "reviewer" } }, AGENTS), ["codex"]);
  assert.deepEqual(stepHarnesses({ assignee: { agent: "gone" } }, AGENTS), [], "an agent nobody defined");
  assert.deepEqual(stepHarnesses({ assignee: { input: "who" } }, AGENTS), [], "an input is known when the run starts");
  assert.deepEqual(stepHarnesses({ assignee: { team: "delivery" } }, AGENTS), []);
  assert.deepEqual(stepHarnesses({ assignee: { human: "ab".repeat(32) } }, AGENTS), []);
  assert.deepEqual(stepHarnesses({ assignee: null }, AGENTS), []);
  assert.deepEqual(stepHarnesses({}, null), []);
  assert.deepEqual(stepHarnesses(null, AGENTS), []);
});

test("the sentence under the pin says who decides, and what runs once a level is fitted", () => {
  const five = { available: ["low", "medium", "high", "xhigh", "max"], known: true };
  const four = { available: ["low", "medium", "high", "max"], known: true };
  assert.equal(stepEffortHint({}, five), "Inherit leaves it to the agent that runs the step.");
  assert.equal(stepEffortHint({ effort: null }, five), "Inherit leaves it to the agent that runs the step.");
  assert.equal(stepEffortHint({ effort: "xhigh" }, five), "Runs at Extra high — the step's pin.");
  assert.equal(stepEffortHint({ effort: "xhigh" }, four), "Runs at High, the nearest to Extra high that is taken — the step's pin.");
  assert.equal(stepEffortHint({ effort: "auto" }, five), "The Decision-Making Agent names the level for each task; the agent's own runs while it is off, unsure or does not answer.");
  // Nobody can say which harness runs it: the level as it is asked.
  assert.equal(stepEffortHint({ effort: "minimal" }, { available: [], known: false }), "Runs at Minimal — the step's pin.");
  assert.equal(stepEffortHint({ effort: "minimal" }), "Runs at Minimal — the step's pin.");
  assert.equal(stepEffortHint(null), "Inherit leaves it to the agent that runs the step.");
});

test("where no level is listed the sentence says so, pinned or not", () => {
  const none = { available: [], known: true };
  for (const step of [{}, { effort: "high" }, { effort: "auto" }]) assert.equal(stepEffortHint(step, none), "No effort levels to choose from here.");
});

test("the pin is a field the core's agent step takes, beside its model, and a problem the designer can word", () => {
  const core = readFileSync(new URL("../../../../../crates/bisa-core/src/workflow.rs", import.meta.url), "utf8");
  const at = core.indexOf('"agent" => &[');
  assert.ok(at >= 0, "StepKind::fields_of names the agent step's fields");
  const fields = [...core.slice(at, core.indexOf("]", at)).matchAll(/"([a-z_]+)"/g)].map((m) => m[1]).slice(1);
  assert.ok(fields.includes("model") && fields.includes("effort"), `the agent step takes a model and an effort: ${fields.join(", ")}`);
  assert.match(core, /\n {4}UnsupportedEffort,?\n/, "a pin no harness can take is a problem by name");
  const form = readFileSync(new URL("./AgentStepForm.tsx", import.meta.url), "utf8");
  assert.ok(form.includes("value={step.effort}") && form.includes("withEffort(step, effort)"), "the form reads the pin and writes it");
  assert.ok(form.indexOf("step.model") < form.indexOf("step.effort"), "beside the model pin, after it");
});

test("the harness field is a fallback order as typed: each harness once, in the order given, a blank left out — and the field shows the list it was read into", () => {
  assert.deepEqual(harnessesFrom("claude-code, codex"), ["claude-code", "codex"]);
  assert.deepEqual(harnessesFrom(" codex ,claude-code,, codex , "), ["codex", "claude-code"], "the first mention keeps its place");
  assert.deepEqual(harnessesFrom("acp:goose"), ["acp:goose"]);
  assert.deepEqual(harnessesFrom("claude-code,"), ["claude-code"], "a comma typed and nothing after it yet");
  assert.deepEqual(harnessesFrom(""), [], "nothing typed is no list: the agent's own harness runs");
  assert.deepEqual(harnessesFrom("  ,  "), []);
  assert.deepEqual(harnessesFrom(null), []);
  assert.equal(harnessText({ harness: ["claude-code", "codex"] }), "claude-code, codex");
  assert.equal(harnessText({ harness: [] }), "");
  assert.equal(harnessText({}), "");
  assert.equal(harnessText(null), "");
  for (const typed of ["claude-code, codex", "codex", ""]) assert.equal(harnessText({ harness: harnessesFrom(typed) }), typed, "what is read is what is shown");
  // The field is a draft while it is typed: a list re-written on every keystroke cannot be typed a comma into.
  const form = readFileSync(new URL("./AgentStepForm.tsx", import.meta.url), "utf8");
  assert.ok(form.includes("onChange={(e) => setHarnessDraft(e.target.value)}") && form.includes("onBlur={commitHarness}"), "typed into a draft, read when the field is left");
  assert.ok(!form.includes('.split(",")'), "the form reads the list through the model");
});

test("the picker is told what any of the step's harnesses takes, once every one of them answered; one that did not is unknown, never none", () => {
  assert.deepEqual(stepEfforts([CLAUDE_CODE], "claude-opus-5-5[1m]"), { available: ["low", "medium", "high", "xhigh", "max"], known: true });
  assert.deepEqual(stepEfforts([CLAUDE_CODE], null), { available: ["low", "medium", "high", "max"], known: true }, "no pin: what the harness takes for any model");
  assert.deepEqual(stepEfforts([CODEX, CLAUDE_CODE], "claude-opus-5-5[1m]"), { available: ["low", "medium", "high", "xhigh", "max"], known: true }, "several harnesses: what any of them takes");
  assert.deepEqual(stepEfforts([CLAUDE_CODE], "claude-haiku-4-5"), { available: [], known: true }, "a model that takes none: known, and none");
  assert.deepEqual(stepEfforts([GOOSE], null), { available: [], known: true });
  // One that did not answer may take more than the ones that did: nothing is narrowed by half an answer.
  assert.deepEqual(stepEfforts([CODEX, null], null), { available: [], known: false });
  assert.deepEqual(stepEfforts([null], "claude-opus-5-5[1m]"), { available: [], known: false });
  assert.deepEqual(stepEfforts([], "claude-opus-5-5[1m]"), { available: [], known: false }, "nobody can name a harness before the run");
  assert.deepEqual(stepEfforts(null, null), { available: [], known: false });
});

test("an answer is the step's only while the step still names the harnesses it was asked about", () => {
  const read = { asked: "claude-code,codex", answers: [CLAUDE_CODE, CODEX] };
  assert.equal(answersFor("claude-code,codex", read), read.answers);
  assert.deepEqual(answersFor("codex", read), [], "the step moved on while the read was out: the answer is the last step's");
  assert.deepEqual(answersFor("", read), []);
  assert.deepEqual(answersFor("claude-code,codex", null), [], "nothing read yet");
  assert.deepEqual(answersFor("claude-code,codex", undefined), []);
  const form = readFileSync(new URL("./AgentStepForm.tsx", import.meta.url), "utf8");
  assert.ok(form.includes("stepEfforts(answersFor(asked, read.data), step.model)"), "the form reads both through the model");
  assert.ok(!/\.catch\(\(\) => null\)/.test(form), "a harness that does not answer is said in the log, never swallowed");
});
