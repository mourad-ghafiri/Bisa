/**
 * Effort as a person meets it (06 §Effort): an agent on the platform's
 * default plan runs at the setting's level; its plan is given an effort of
 * its own, then one of its models, then a workflow step pins one — each
 * winning in turn; *Auto* hands the level to the Decision-Making Agent and
 * says what runs while it cannot be asked; and a level a model does not take
 * is fitted, the saved value staying in its picker. Every step reads the
 * models the screens draw from; the wiring is read from the sources, as every
 * scenario here reads it — no DOM. Run with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/scenarios/effort.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { modelWords } from "../ui/modelWords.mjs";
import { POINT_IDS, POINTS, fieldsFor, KEYS as DECISION_KEYS } from "../views/_settings/decisionsModel.mjs";
import { draftOf, emptyDraft, saveOf } from "../views/_work/agentDraftModel.mjs";
import { KEYS as SECURITY_KEYS, classifierFieldsFor } from "../views/_settings/securityRules.mjs";
import { EFFORT_SETTING, effortOptions, effortSetting, effortWords, effortsAcross, effortsFor, offersEffort, planEfforts, resolveEffort, withEffort } from "../views/_work/effortModel.mjs";
import { PROJECT_AGENT_KEYS } from "../views/_work/projectSettingsModel.mjs";
import { transcriptTitle } from "../views/_work/sessionTranscriptModel.mjs";
import { workstreamSessionRows } from "../views/_workbench/workstreamSessionsModel.mjs";
import { addModel, answeredBy, healthChips, healthOf, moveModel, patchModel, removeModel, strategyOf } from "../views/_work/modelPlanModel.mjs";
import { answersFor, harnessesFrom, stepEffortHint, stepEfforts, stepHarnesses } from "../views/_workflow/forms/agentStepModel.mjs";
import { PROBLEM_KIND_LABEL, blankStep } from "../views/_workflow/stepKinds.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const values = (options) => options.map((o) => o.value);

const OPUS = "claude-opus-5-5[1m]";
const SONNET = "claude-sonnet-5-5[1m]";
const EARLIER = "claude-sonnet-4-6";

/** `GET /harnesses/claude-code/models`, as the node answers it: the levels for an id it does not list, and each model's own. */
const CLAUDE_CODE = {
  harness: "claude-code",
  efforts: ["low", "medium", "high", "max"],
  models: [
    { id: OPUS, efforts: ["low", "medium", "high", "xhigh", "max"] },
    { id: SONNET, efforts: ["low", "medium", "high", "xhigh", "max"] },
    { id: EARLIER, efforts: ["low", "medium", "high", "max"] },
    { id: "claude-haiku-4-5", efforts: [] },
  ],
};
const CODEX = { harness: "codex", efforts: ["low", "medium", "high"], models: [] };

/** What every built-in agent opens with: Opus, then Sonnet, and no effort of its own. */
const DEFAULT_PLAN = { strategy: "fallback", models: [{ model: OPUS, weight: 1, enabled: true }, { model: SONNET, weight: 1, enabled: true }] };

/** The sentence under one model's row of the plan editor. */
const rowHint = (plan, model, setting, harness = CLAUDE_CODE) => {
  const own = plan.models.find((m) => m.model === model)?.effort;
  return effortWords(resolveEffort(null, own, plan.effort, setting.setting), { available: effortsFor(harness, model), known: true, origin: setting.origin }).hint;
};

test("an agent on the default plan runs at the setting's level, and its editor offers what its models take", () => {
  const setting = effortSetting([{ key: EFFORT_SETTING, value: "high", origin: "default" }]);
  assert.deepEqual(setting, { setting: "high", origin: "default" });
  assert.equal(DEFAULT_PLAN.effort, undefined, "a built-in agent names no effort: the setting is one dial for all of them");

  // The plan's own picker: Inherit, Auto, then every level one of its models takes.
  const planTaken = planEfforts(CLAUDE_CODE, DEFAULT_PLAN);
  const offered = effortOptions(planTaken, { inherit: true, auto: true, known: true, current: DEFAULT_PLAN.effort });
  assert.deepEqual(values(offered), ["", "auto", "low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(offered.map((o) => o.label), ["Inherit", "Auto", "Low", "Medium", "High", "Extra high", "Max"]);
  assert.equal(effortWords(resolveEffort(null, null, DEFAULT_PLAN.effort, setting.setting), { available: planTaken, known: true, origin: setting.origin }).hint, "Runs at High — the default.");
  for (const model of [OPUS, SONNET]) assert.equal(rowHint(DEFAULT_PLAN, model, setting), "Runs at High — the default.");

  // The workspace turns the dial: every agent that names nothing follows.
  const workspace = effortSetting([{ key: EFFORT_SETTING, value: "medium", origin: "workspace" }]);
  assert.equal(rowHint(DEFAULT_PLAN, OPUS, workspace), "Runs at Medium — the workspace's setting.");
});

test("the agent's effort, then a model's own, then a step's pin — each wins in turn", () => {
  const setting = effortSetting([{ key: EFFORT_SETTING, value: "high", origin: "workspace" }]);

  // 1. The agent: its plan is given an effort of its own.
  let plan = withEffort(DEFAULT_PLAN, "max");
  assert.equal(plan.effort, "max");
  assert.deepEqual(plan.models, DEFAULT_PLAN.models, "the models are as they were");
  assert.equal(rowHint(plan, OPUS, setting), "Runs at Max — the agent's.");
  assert.equal(rowHint(plan, SONNET, setting), "Runs at Max — the agent's.");

  // 2. One model: Opus is given its own, and Sonnet keeps the agent's.
  plan = { ...plan, models: plan.models.map((m) => (m.model === OPUS ? withEffort(m, "low") : m)) };
  assert.equal(rowHint(plan, OPUS, setting), "Runs at Low — this model's own.");
  assert.equal(rowHint(plan, SONNET, setting), "Runs at Max — the agent's.");
  assert.deepEqual(plan, {
    strategy: "fallback",
    effort: "max",
    models: [{ model: OPUS, weight: 1, enabled: true, effort: "low" }, { model: SONNET, weight: 1, enabled: true }],
  }, "what is saved: the wire's words, and no field where nothing was chosen");

  // 3. A workflow's step pins one: it decides before the model's and the agent's.
  const step = withEffort({ ...blankStep("agent", "build"), assignee: { agent: "developer" }, model: OPUS }, "xhigh");
  assert.equal(step.effort, "xhigh");
  assert.deepEqual(resolveEffort(step.effort, "low", plan.effort, setting.setting), { kind: "level", level: "xhigh", from: "step" });
  const harnesses = stepHarnesses(step, [{ id: "developer", harness: "claude-code" }]);
  assert.deepEqual(harnesses, ["claude-code"], "the step runs on its agent's harness");
  const taken = effortsAcross([CLAUDE_CODE], step.model);
  assert.equal(stepEffortHint(step, { available: taken, known: true }), "Runs at Extra high — the step's pin.");

  // Inherit takes each back, one at a time.
  const unpinned = withEffort(step, null);
  assert.ok(!("effort" in unpinned), "no pin: the agent that runs the step decides");
  assert.equal(stepEffortHint(unpinned, { available: taken, known: true }), "Inherit leaves it to the agent that runs the step.");
  assert.deepEqual(resolveEffort(unpinned.effort, "low", plan.effort, setting.setting), { kind: "level", level: "low", from: "model" });
  const inherited = { ...withEffort(plan, null), models: plan.models.map((m) => withEffort(m, null)) };
  assert.deepEqual(inherited, DEFAULT_PLAN, "back to the default plan, field for field");
  assert.equal(rowHint(inherited, OPUS, setting), "Runs at High — the workspace's setting.");
});

test("Auto hands the level to the Decision-Making Agent, and says what runs while it cannot be asked", () => {
  const setting = effortSetting([{ key: EFFORT_SETTING, value: "high", origin: "workspace" }]);
  const plan = withEffort(DEFAULT_PLAN, "auto");
  const resolved = resolveEffort(null, null, plan.effort, setting.setting);
  assert.deepEqual(resolved, { kind: "auto", fallback: "high", from: "plan", fallbackFrom: "setting" });
  assert.deepEqual(effortWords(resolved, { available: effortsFor(CLAUDE_CODE, OPUS), known: true, origin: setting.origin }), {
    runs: "high",
    label: "Auto",
    hint: "The Decision-Making Agent names the level for each task; High runs while it is off, unsure or does not answer.",
  });

  // A model's own level under an agent on Auto: the level, and nobody is asked for that model.
  const mixed = { ...plan, models: plan.models.map((m) => (m.model === SONNET ? withEffort(m, "medium") : m)) };
  assert.equal(rowHint(mixed, SONNET, setting), "Runs at Medium — this model's own.");
  assert.match(rowHint(mixed, OPUS, setting), /^The Decision-Making Agent names the level/);

  // The setting itself on Auto, with nothing further down: the default is the fallback.
  const auto = effortSetting([{ key: EFFORT_SETTING, value: "auto", origin: "workspace" }]);
  assert.deepEqual(resolveEffort(null, null, null, auto.setting), { kind: "auto", fallback: "high", from: "setting", fallbackFrom: "default" });

  // A step pinned to Auto cannot know its fallback before it is assigned: the agent's own.
  assert.equal(stepEffortHint(withEffort(blankStep("agent", "build"), "auto"), { available: [], known: false }), "The Decision-Making Agent names the level for each task; the agent's own runs while it is off, unsure or does not answer.");

  // It is the eleventh decision point, second in the list, and selected where it is used.
  assert.equal(POINT_IDS.length, 11);
  assert.deepEqual(POINT_IDS.slice(0, 3), ["model.route", "model.effort", "security.tool"]);
  assert.equal(POINTS["model.effort"].label, "How hard a model works");
});

test("a level a model does not take is fitted, and the value saved stays in its picker", () => {
  const setting = effortSetting([{ key: EFFORT_SETTING, value: "high", origin: "workspace" }]);

  // The plan asks for Extra high; an earlier model in it takes no such level.
  const plan = withEffort({ strategy: "fallback", models: [{ model: OPUS, weight: 1, enabled: true }, { model: EARLIER, weight: 1, enabled: true }] }, "xhigh");
  assert.equal(rowHint(plan, OPUS, setting), "Runs at Extra high — the agent's.");
  assert.equal(rowHint(plan, EARLIER, setting), "Runs at High, the nearest to Extra high that is taken — the agent's.");

  // That model's own picker offers only what it takes — and keeps a value saved before, marked.
  const earlier = withEffort(plan.models[1], "xhigh");
  const kept = effortOptions(effortsFor(CLAUDE_CODE, EARLIER), { inherit: true, auto: true, known: true, current: earlier.effort });
  assert.deepEqual(values(kept), ["", "auto", "low", "medium", "high", "max", "xhigh"]);
  assert.deepEqual(kept.at(-1), { value: "xhigh", label: "Extra high — fitted to High at launch", kept: true });
  assert.equal(earlier.effort, "xhigh", "nothing rewrote what was saved");

  // The agent moves to Codex: three levels for any model, the plan's Extra high fitted down to High.
  assert.deepEqual(planEfforts(CODEX, plan), ["low", "medium", "high"]);
  assert.equal(rowHint(plan, OPUS, setting, CODEX), "Runs at High, the nearest to Extra high that is taken — the agent's.");
  assert.deepEqual(values(effortOptions(planEfforts(CODEX, plan), { inherit: true, auto: true, known: true, current: plan.effort })), ["", "auto", "low", "medium", "high", "xhigh"]);

  // A model with no control, on a harness whose others have one: nothing to pick, and the row says so.
  const haiku = effortOptions(effortsFor(CLAUDE_CODE, "claude-haiku-4-5"), { inherit: true, auto: true, known: true, current: null });
  assert.equal(offersEffort(haiku), false);
  assert.equal(effortWords(resolveEffort(null, null, plan.effort, setting.setting), { available: effortsFor(CLAUDE_CODE, "claude-haiku-4-5"), known: true, origin: setting.origin }).hint, "No effort levels to choose from here.");

  // A harness that has not answered yet is unknown, not none: every level, and the level as asked.
  const unknown = effortOptions(effortsFor(null, OPUS), { inherit: true, auto: true, known: false, current: null });
  assert.deepEqual(values(unknown), ["", "auto", "minimal", "low", "medium", "high", "xhigh", "max"]);
  assert.equal(effortWords(resolveEffort(null, null, "minimal", setting.setting), { available: [], known: false, origin: setting.origin }).hint, "Runs at Minimal — the agent's.");

  // A step that names two harnesses is offered what either takes; one no harness can set is a problem by name.
  const step = withEffort({ ...blankStep("agent", "build"), harness: ["codex", "claude-code"], model: EARLIER }, "max");
  assert.deepEqual(stepHarnesses(step, []), ["codex", "claude-code"]);
  assert.deepEqual(effortsAcross([CODEX, CLAUDE_CODE], step.model), ["low", "medium", "high", "max"]);
  assert.equal(PROBLEM_KIND_LABEL.unsupported_effort, "pins an effort none of its harnesses can set");
});

test("a plan is edited and saved whole: a model added once, moved to lead, given its own effort; a quota wall is read off the ledger; the harness changed is asked again", () => {
  // The agent opens on the plan it ships with; the harness has answered.
  const agent = { id: "scout", name: "Scout", system_prompt: "You scout.", harness: "claude-code", models: DEFAULT_PLAN, respond: "owner_only", skills: [], mcps: [], tags: [] };
  let draft = draftOf(agent);
  let answered = answeredBy(draft.harness, CLAUDE_CODE, null);
  assert.equal(strategyOf(draft.plan).label, "Fallback");
  assert.deepEqual(values(effortOptions(planEfforts(answered, draft.plan), { inherit: true, auto: true, known: answered !== null, current: draft.plan.effort })), ["", "auto", "low", "medium", "high", "xhigh", "max"]);

  // An earlier Sonnet is added — once: a second press says so and adds nothing.
  let added = addModel(draft.plan, EARLIER);
  draft = { ...draft, plan: added.plan };
  added = addModel(draft.plan, EARLIER);
  assert.equal(added.error, `${EARLIER} is already in this plan.`);
  assert.deepEqual(draft.plan.models.map((m) => m.model), [OPUS, SONNET, EARLIER]);

  // It is moved up to lead, and told to work harder than it can: fitted, and said so under its row.
  draft = { ...draft, plan: moveModel(moveModel(draft.plan, 2, -1), 1, -1) };
  assert.deepEqual(draft.plan.models.map((m) => m.model), [EARLIER, OPUS, SONNET]);
  draft = { ...draft, plan: { ...draft.plan, models: draft.plan.models.map((m, i) => (i === 0 ? withEffort(m, "xhigh") : m)) } };
  assert.equal(rowHint(draft.plan, EARLIER, { setting: "high", origin: "workspace" }), "Runs at High, the nearest to Extra high that is taken — this model's own.");
  assert.equal(rowHint(draft.plan, OPUS, { setting: "high", origin: "workspace" }), "Runs at High — the workspace's setting.", "the others still inherit");

  // Sonnet is switched off without losing its place; Opus hits a quota wall, and its row says how long is left.
  draft = { ...draft, plan: patchModel(draft.plan, 2, { enabled: false }) };
  assert.deepEqual(draft.plan.models.map((m) => [m.model, m.enabled]), [[EARLIER, true], [OPUS, true], [SONNET, false]]);
  const ledger = [{ harness: "claude-code", model: OPUS, retry_in_secs: 240, consecutive_failures: 1, in_flight: 0 }, { harness: "codex", model: "codex default", consecutive_failures: 0, in_flight: 2 }];
  assert.deepEqual(healthChips(healthOf(ledger, draft.harness, OPUS)).map((c) => c.words), ["cooling · retry in 4m", "1 failure"]);
  assert.deepEqual(healthChips(healthOf(ledger, draft.harness, EARLIER)), [], "a healthy idle model wears nothing");

  // The save is the whole definition, the plan as it stands.
  const save = saveOf(draft, agent, false);
  assert.equal(save.kind, "patch");
  assert.deepEqual(save.body.models, { strategy: "fallback", models: [{ model: EARLIER, weight: 1, enabled: true, effort: "xhigh" }, { model: OPUS, weight: 1, enabled: true }, { model: SONNET, weight: 1, enabled: false }] });

  // The harness is changed to Codex: until Codex answers, Claude Code's answer is nobody's — unknown, never none.
  draft = { ...draft, harness: "codex" };
  answered = answeredBy(draft.harness, CLAUDE_CODE, null);
  assert.equal(answered, null);
  assert.deepEqual(values(effortOptions(planEfforts(answered, draft.plan), { inherit: true, auto: true, known: false, current: draft.plan.effort })), ["", "auto", "minimal", "low", "medium", "high", "xhigh", "max"], "every level may be asked for while nobody has said");
  answered = answeredBy(draft.harness, CODEX, null);
  assert.deepEqual(values(effortOptions(effortsFor(answered, EARLIER), { inherit: true, auto: true, known: true, current: "xhigh" })), ["", "auto", "low", "medium", "high", "xhigh"], "Codex takes three; the level saved stays in its picker");
  assert.deepEqual(healthChips(healthOf(ledger, "codex", null)).map((c) => c.words), ["2 in flight"], "a plan emptied runs the harness's own, and the ledger knows it by the engine's name");
  draft = { ...draft, plan: removeModel(removeModel(removeModel(draft.plan, 0), 0), 0) };
  assert.deepEqual(draft.plan.models, []);
  assert.equal("models" in saveOf({ ...draft, plan: { strategy: "fallback", models: [] } }, null, false).body, false, "a new agent with an empty plan names none");
});

test("a step names its harnesses as it is typed, and its picker waits for every one of them", () => {
  // The field is left after "claude-code, codex" was typed: two harnesses, in that order.
  const step = { ...blankStep("agent", "build"), harness: harnessesFrom("claude-code, codex"), model: OPUS };
  assert.deepEqual(stepHarnesses(step, []), ["claude-code", "codex"]);
  const asked = stepHarnesses(step, []).join(",");
  // Claude Code answered and Codex has not: nothing is narrowed by half an answer.
  let facts = stepEfforts(answersFor(asked, { asked, answers: [CLAUDE_CODE, null] }), step.model);
  assert.deepEqual(facts, { available: [], known: false });
  assert.deepEqual(values(effortOptions(facts.available, { inherit: true, auto: true, known: facts.known, current: step.effort })), ["", "auto", "minimal", "low", "medium", "high", "xhigh", "max"]);
  // Both answered: what either takes for the pinned model.
  facts = stepEfforts(answersFor(asked, { asked, answers: [CLAUDE_CODE, CODEX] }), step.model);
  assert.deepEqual(facts, { available: ["low", "medium", "high", "xhigh", "max"], known: true });
  assert.equal(stepEffortHint(withEffort(step, "minimal"), facts), "Runs at Low, the nearest to Minimal that is taken — the step's pin.");
  // The step is moved to another harness while the read is out: the old answer is nobody's.
  const moved = { ...step, harness: harnessesFrom("goose") };
  assert.deepEqual(stepEfforts(answersFor(stepHarnesses(moved, []).join(","), { asked, answers: [CLAUDE_CODE, CODEX] }), moved.model), { available: [], known: false });
});

test("a session's row says the effort it runs at, beside its model, only when the row carries one", () => {
  const sessions = [
    { id: "s1", kind: "worker", workstream: "w1", harness: "claude-code", agent: "developer", model: OPUS, effort: "high", state: { state: "thinking" }, since: 5, started: 1, children: [] },
    { id: "s2", kind: "worker", workstream: "w1", harness: "goose", agent: "reviewer", model: "gpt-5.5", state: { state: "idle" }, since: 6, started: 2, children: [] },
  ];
  const rows = workstreamSessionRows(sessions, [], "w1");
  const words = rows.map((r) => modelWords(r.model, r.effort)?.short);
  assert.deepEqual(words, ["claude-opus-5-5[1m] · high", "gpt-5.5"], "the level after the model, and none where the harness was sent none");
  assert.equal(transcriptTitle(sessions[0]), "developer · claude-code · claude-opus-5-5[1m] · high");
  assert.equal(transcriptTitle(sessions[1]), "reviewer · goose · gpt-5.5");
});

test("the settings that name an effort have their homes", () => {
  assert.ok(PROJECT_AGENT_KEYS.includes(EFFORT_SETTING), "a project's own, under About › Settings");
  assert.deepEqual(fieldsFor("harness"), [DECISION_KEYS.harness.id, DECISION_KEYS.harness.model, DECISION_KEYS.harness.effort], "the judge's own, beside its model");
  assert.deepEqual(classifierFieldsFor("harness"), [SECURITY_KEYS.classifier.harness, SECURITY_KEYS.classifier.model, SECURITY_KEYS.classifier.effort], "the classifier's own, beside its model");
  const registry = src("../../../crates/bisa-core/src/settings.rs");
  for (const key of [EFFORT_SETTING, DECISION_KEYS.harness.effort, SECURITY_KEYS.classifier.effort]) assert.ok(registry.includes(`"${key}",`), `${key} is registered`);
  // The two defaults that name a model name the platform's own.
  for (const key of [DECISION_KEYS.harness.model, SECURITY_KEYS.classifier.model]) {
    const at = registry.indexOf(`"${key}",`);
    assert.ok(registry.slice(at, at + 200).includes('json!("claude-sonnet-5-5[1m]")'), `${key} defaults to the judge's model`);
  }
});

test("the wiring: the editor, the step form and the rows read the one model", () => {
  const editor = src("../views/_work/ModelPlanEditor.tsx");
  assert.ok(editor.includes('from "./effortModel.mjs"') && editor.includes('from "./EffortPicker"'), "the plan editor draws the model's facts");
  assert.ok(editor.includes("value={plan.effort}") && editor.includes("onChange(withEffort(plan, effort))"), "the plan's own effort");
  assert.ok(editor.includes("value={m.effort}") && editor.includes("withEffort(c, effort)"), "and one per model");
  assert.ok(editor.includes("effortsFor(answered, m.model)"), "each model is offered what it takes on this harness");
  assert.ok(editor.includes("answeredBy(harness, modelData, modelsError)"), "and never another harness's answer");
  assert.equal(editor.split("{planWords.hint}").length, 2, "the sentence under the plan's control");
  assert.ok(editor.includes("{words.hint}"), "and under each model's");

  const picker = src("../views/_work/EffortPicker.tsx");
  assert.ok(picker.includes("<Select") && picker.includes("aria-label={label}"), "a native select: the keyboard reaches it and a reader names it");
  assert.ok(picker.includes("if (!offersEffort(options)) return null;"), "nothing to pick draws nothing");
  assert.ok(picker.includes("useResolvedSettings(project)") && !picker.includes("localStorage"), "the setting is read through the settings store; nothing of it is kept here");

  const agentEditor = src("../views/_work/AgentEditor.tsx");
  assert.ok(agentEditor.includes("draftOf(agent)") && agentEditor.includes("saveOf(d, agent, core)"), "the editor's draft and its save are the draft model's");
  const opened = draftOf({ id: "scout", name: "Scout", system_prompt: "…", harness: "claude-code", models: { strategy: "fallback", models: [{ model: OPUS }], effort: "xhigh" }, respond: "owner_only", skills: [], mcps: [], tags: [] });
  assert.equal(opened.plan.effort, "xhigh", "an agent opened for editing keeps its plan's effort");
  assert.deepEqual(saveOf({ ...emptyDraft("claude-code"), name: "Scout", plan: { strategy: "fallback", models: [], effort: "auto" } }, null, false).body.models, { strategy: "fallback", models: [], effort: "auto" }, "a new agent's plan is sent when it names an effort alone");
  assert.equal("models" in saveOf({ ...emptyDraft("claude-code"), name: "Scout" }, null, false).body, false, "and not when it names nothing");

  const agents = src("../views/Agents.tsx");
  assert.ok(agents.includes('t("screens-agents-effort")') && agents.includes("effortOf(m.model, m.effort)"), "the agent's page says the plan's effort and each model's");

  const form = src("../views/_workflow/forms/AgentStepForm.tsx");
  assert.ok(form.includes("value={step.effort}") && form.includes("onChange(withEffort(step, effort))"), "the step form carries the pin");
  assert.ok(form.includes("stepHarnesses(step, ws.agents)") && form.includes("stepEfforts(answersFor(asked, read.data), step.model)"), "offering what the step's harnesses take for its model");

  const api = src("../api.ts");
  assert.ok(api.includes("get<{ harness: string; efforts: Effort[]; models: ModelInfo[] }>(`/harnesses/${harness}/models`, s)"), "the models route answers the levels");
  const hand = src("../types.hand.ts");
  assert.match(hand, /export interface ModelInfo \{[^}]*efforts: Effort\[\];/, "and each model its own");
  assert.ok(!/^export type Effort(Choice)? =/m.test(hand), "the two words are the core's, generated — never declared by hand");

  for (const row of ["../views/_work/WorkstreamSessions.tsx", "../views/_workbench/rail/RailAgentRow.tsx", "../views/_work/sessionTranscriptModel.mjs"]) {
    assert.match(src(row), /modelWords\((row\.)?model, (row\.)?effort\)/, `${row} says the effort through the one function`);
  }
});
