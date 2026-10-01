/**
 * An agent's model plan as the editor and the card read it: the strategies
 * are the core's, the ledger is asked by the key the engine files under, a
 * badge shows only what is worth reading, and every edit is a new plan. Run
 * with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/views/_work/modelPlanModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { DEFAULT_STRATEGY, MAX_WEIGHT, STRATEGIES, addModel, answeredBy, defaultModelKey, enabledCount, healthChips, healthOf, moveModel, patchModel, removeModel, strategyOf, unusedModels, weightFrom, weightWords } from "./modelPlanModel.mjs";

const rust = (path) => readFileSync(new URL(`../../../../crates/${path}`, import.meta.url), "utf8");
const src = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const snake = (word) => word.replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase();

const OPUS = "claude-opus-5-5[1m]";
const SONNET = "claude-sonnet-5-5[1m]";
/** The plan every agent the platform ships carries. */
const shipped = () => ({ strategy: "fallback", models: [{ model: OPUS, weight: 1, enabled: true }, { model: SONNET, weight: 1, enabled: true }] });

test("the strategies are the core's, word for word and in its order, the first its default; each says what it does and what order means under it", () => {
  const core = rust("bisa-core/src/model_plan.rs");
  const declared = core.match(/pub enum ModelStrategy \{\n([\s\S]*?)\n\}/);
  assert.ok(declared, "ModelStrategy is declared in model_plan.rs");
  assert.ok(/#\[serde\(rename_all = "snake_case"\)\]\s*pub enum ModelStrategy/.test(core), "its words on the wire are snake_case");
  const variants = [...declared[1].matchAll(/^ {4}([A-Z][A-Za-z]*),$/gm)].map((m) => snake(m[1]));
  assert.deepEqual(STRATEGIES.map((s) => s.value), variants);
  assert.equal(snake(declared[1].match(/#\[default\]\s*([A-Z][A-Za-z]*),/)[1]), DEFAULT_STRATEGY);
  assert.equal(STRATEGIES[0].value, DEFAULT_STRATEGY);
  for (const s of STRATEGIES) for (const word of [s.label, s.explain, s.orderMeans]) assert.ok(word && !word.startsWith("work-"), `${s.value}: a sentence of the catalog`);
  assert.equal(new Set(STRATEGIES.map((s) => s.label)).size, STRATEGIES.length, "no two share a label");
  assert.deepEqual(STRATEGIES.map((s) => s.label), ["Fallback", "Weighted", "Round-robin", "Least busy", "Auto-route"]);
});

test("a plan reads with its strategy's words; one that names none, or a word nobody knows, is the default", () => {
  assert.equal(strategyOf({ strategy: "least_busy" }).label, "Least busy");
  assert.equal(strategyOf({}).value, "fallback");
  assert.equal(strategyOf(null).value, "fallback");
  assert.equal(strategyOf({ strategy: "cheapest" }).value, "fallback");
  assert.equal(strategyOf({ strategy: "" }).value, "fallback");
});

test("the ledger is asked by the key the engine files under: the model's id on its harness, and for an unpinned session the harness's own name for it — built in code, never a word of the catalog", () => {
  const engine = rust("bisa-engine/src/models.rs");
  assert.ok(engine.includes('_ => format!("{harness} default"),'), "model_key names an unpinned session `<harness> default`");
  assert.equal(defaultModelKey("claude-code"), "claude-code default");
  assert.equal(defaultModelKey("acp:goose"), "acp:goose default");
  const model = src("./modelPlanModel.mjs");
  assert.ok(/return `\$\{harness\} default`; \/\/ for the machine/.test(model), "the key is built in code and marked for the machine");
  for (const file of ["./ModelPlanEditor.tsx", "../Agents.tsx"]) {
    const text = src(file);
    assert.ok(!/health(For|Of)\([^)]*\bt[r]?\(/.test(text), `${file}: the ledger is never asked by a sentence of the catalog`);
    assert.ok(text.includes("healthOf("), `${file} asks through the model`);
  }

  const rows = [
    { harness: "claude-code", model: OPUS, consecutive_failures: 2, in_flight: 0, retry_in_secs: 90 },
    { harness: "codex", model: OPUS, consecutive_failures: 0, in_flight: 1 },
    { harness: "claude-code", model: "claude-code default", consecutive_failures: 0, in_flight: 3 },
  ];
  assert.equal(healthOf(rows, "claude-code", OPUS), rows[0]);
  assert.equal(healthOf(rows, "codex", OPUS), rows[1], "a cooldown is true of one harness's account, never of the model alone");
  assert.equal(healthOf(rows, "claude-code", null), rows[2], "a plan that names no model runs the harness's own");
  assert.equal(healthOf(rows, "codex", null), undefined);
  assert.equal(healthOf(rows, "claude-code", SONNET), undefined);
  assert.equal(healthOf(null, "claude-code", OPUS), undefined);
});

test("a badge shows only what is worth reading: a healthy idle model wears none; cooling says how long is left, failures are counted, sessions in flight are counted", () => {
  assert.deepEqual(healthChips(undefined), []);
  assert.deepEqual(healthChips(null), []);
  assert.deepEqual(healthChips({ consecutive_failures: 0, in_flight: 0 }), []);
  assert.deepEqual(healthChips({ consecutive_failures: 0, in_flight: 0, retry_in_secs: 0 }), [], "a cooldown that ran out is none");
  assert.deepEqual(healthChips({ retry_in_secs: 44.2, consecutive_failures: 0, in_flight: 0 }).map((c) => [c.id, c.tone, c.icon, c.words]), [["cooling", "warn", "waiting", "cooling · retry in 45s"]], "a second begun is a second left");
  assert.deepEqual(healthChips({ retry_in_secs: 134, consecutive_failures: 0, in_flight: 0 })[0].words, "cooling · retry in 2m 14s");
  assert.deepEqual(healthChips({ retry_in_secs: 4800, consecutive_failures: 0, in_flight: 0 })[0].words, "cooling · retry in 1h 20m");
  assert.deepEqual(healthChips({ consecutive_failures: 1, in_flight: 0 }).map((c) => [c.id, c.tone, c.words]), [["failures", "warn", "1 failure"]]);
  assert.deepEqual(healthChips({ consecutive_failures: 3, in_flight: 2 }).map((c) => [c.id, c.tone, c.icon, c.words]), [["failures", "warn", "warn", "3 failures"], ["flight", "accent", "working", "2 in flight"]]);
  assert.deepEqual(healthChips({ retry_in_secs: 30, consecutive_failures: 3, in_flight: 1 }).map((c) => [c.id, c.tone]), [["cooling", "warn"], ["failures", "quiet"], ["flight", "accent"]], "beside a cooldown the failures are quiet: the cooldown already says why");
  for (const chip of healthChips({ retry_in_secs: 30, consecutive_failures: 3, in_flight: 1 })) assert.ok(chip.tip.endsWith("."), "each with the sentence that says what it means");
  // A row the node could not count — a field missing, or no number — never throws and never invents a figure.
  assert.deepEqual(healthChips({}), []);
  assert.deepEqual(healthChips({ retry_in_secs: "soon", consecutive_failures: "many", in_flight: null }), []);
});

test("a weight is a whole number from one: nothing, a word, zero or a negative is one — a model is switched off by its switch, never by its weight", () => {
  assert.equal(weightFrom("3"), 3);
  assert.equal(weightFrom(7), 7);
  assert.equal(weightFrom("2.9"), 2);
  assert.equal(weightFrom(""), 1);
  assert.equal(weightFrom("heavy"), 1);
  assert.equal(weightFrom("0"), 1);
  assert.equal(weightFrom(-4), 1);
  assert.equal(weightFrom(undefined), 1);
  assert.equal(weightFrom("1e20"), MAX_WEIGHT, "held to what the wire carries");
  assert.equal(weightFrom(Infinity), 1);
  assert.equal(MAX_WEIGHT, 2 ** 32 - 1);
  assert.ok(/pub weight: u32,/.test(rust("bisa-core/src/model_plan.rs")), "the wire's weight is a u32");
  assert.equal(weightWords(3), "weight 3");
  assert.equal(weightWords(undefined), "weight 1", "a model that names none weighs one");
});

test("an answer is a harness's own only when that harness gave it: one kept from the harness picked before, or a read that failed, is unknown — never none", () => {
  const claude = { harness: "claude-code", efforts: ["low"], models: [] };
  assert.equal(answeredBy("claude-code", claude), claude);
  assert.equal(answeredBy("codex", claude), null, "the answer on screen is the last harness's while this one is asked");
  assert.equal(answeredBy("claude-code", claude, "the node did not answer"), null);
  assert.equal(answeredBy("claude-code", null), null);
  assert.equal(answeredBy("claude-code", undefined), null);
  assert.equal(answeredBy("", { harness: "", efforts: [], models: [] }), null, "no harness picked is nobody to ask");
});

test("a model is added once, at the end, enabled and weighing one; an id already in the plan is said and never added twice", () => {
  const plan = shipped();
  const added = addModel(plan, "  claude-haiku-4-5 ");
  assert.equal(added.error, null);
  assert.deepEqual(added.plan.models.map((m) => m.model), [OPUS, SONNET, "claude-haiku-4-5"]);
  assert.deepEqual(added.plan.models[2], { model: "claude-haiku-4-5", weight: 1, enabled: true });
  assert.equal(added.plan.strategy, "fallback");
  assert.deepEqual(plan, shipped(), "the plan handed in is left alone");
  assert.deepEqual(addModel(plan, ` ${SONNET} `), { plan: null, error: `${SONNET} is already in this plan.` });
  assert.deepEqual(addModel(plan, "   "), { plan: null, error: null }, "nothing typed is nothing to add, and nothing to say");
  assert.deepEqual(addModel({ strategy: "fallback" }, OPUS).plan.models, [{ model: OPUS, weight: 1, enabled: true }], "into a plan that named no model");
  // The plan's own effort rides along untouched.
  assert.equal(addModel({ ...plan, effort: "auto" }, "x").plan.effort, "auto");
});

test("a model moves one place at a time and stays inside the plan; it keeps what it carries", () => {
  const plan = { ...shipped(), models: [{ model: OPUS, weight: 3, enabled: true, effort: "max" }, { model: SONNET, weight: 1, enabled: false }, { model: "c", weight: 1, enabled: true }] };
  assert.deepEqual(moveModel(plan, 0, 1).models.map((m) => m.model), [SONNET, OPUS, "c"]);
  assert.deepEqual(moveModel(plan, 0, 1).models[1], { model: OPUS, weight: 3, enabled: true, effort: "max" });
  assert.deepEqual(moveModel(plan, 2, -1).models.map((m) => m.model), [OPUS, "c", SONNET]);
  assert.equal(moveModel(plan, 0, -1), plan, "the first has nowhere to go up");
  assert.equal(moveModel(plan, 2, 1), plan, "the last nowhere to go down");
  assert.equal(moveModel(plan, 7, -1), plan);
  assert.equal(moveModel(plan, 1, 0), plan);
  assert.equal(moveModel(plan, 0.5, 1), plan);
  assert.equal(moveModel({ strategy: "fallback" }, 0, 1).models, undefined, "a plan with no models is left as it is");
  assert.deepEqual(plan.models.map((m) => m.model), [OPUS, SONNET, "c"], "the plan handed in is left alone");
});

test("a model is switched off without losing its place, weighted, told what it is best for, and removed — each a new plan, the others the same objects", () => {
  const plan = shipped();
  const off = patchModel(plan, 0, { enabled: false });
  assert.deepEqual(off.models, [{ model: OPUS, weight: 1, enabled: false }, { model: SONNET, weight: 1, enabled: true }]);
  assert.equal(off.models[1], plan.models[1], "the model beside it is untouched");
  assert.notEqual(off, plan);
  assert.deepEqual(patchModel(plan, 1, { weight: 3, suited_for: "quick edits" }).models[1], { model: SONNET, weight: 3, enabled: true, suited_for: "quick edits" });
  assert.equal(patchModel(plan, 5, { enabled: false }), plan);
  assert.equal(patchModel(plan, -1, { enabled: false }), plan);
  assert.deepEqual(removeModel(plan, 0).models, [{ model: SONNET, weight: 1, enabled: true }]);
  assert.deepEqual(removeModel(removeModel(plan, 0), 0).models, [], "an empty plan is the honest way to say whatever the harness runs by default");
  assert.equal(removeModel(plan, 2), plan);
  assert.deepEqual(plan, shipped());
});

test("what a harness offers is offered until the plan holds it; the count is of the models that may run", () => {
  const offered = [{ id: OPUS }, { id: SONNET }, { id: "claude-haiku-4-5", label: "Haiku 4.5" }];
  assert.deepEqual(unusedModels(offered, shipped()), [{ id: "claude-haiku-4-5", label: "Haiku 4.5" }]);
  assert.deepEqual(unusedModels(offered, { strategy: "fallback" }), offered);
  assert.deepEqual(unusedModels(null, shipped()), []);
  assert.equal(enabledCount(shipped()), 2);
  assert.equal(enabledCount(patchModel(shipped(), 0, { enabled: false })), 1);
  assert.equal(enabledCount({ models: [{ model: "a" }] }), 1, "a model that does not say is on");
  assert.equal(enabledCount(null), 0);
});

test("the wiring: the editor and the card read the one model, and neither spells a rule of its own", () => {
  const editor = src("./ModelPlanEditor.tsx");
  const card = src("../Agents.tsx");
  for (const fn of ["addModel(", "moveModel(", "patchModel(", "removeModel(", "weightFrom(", "unusedModels(", "enabledCount(", "healthChips(", "answeredBy(", "strategyOf("]) assert.ok(editor.includes(fn), `the editor reads ${fn}`);
  for (const fn of ["strategyOf(", "answeredBy(", "healthOf(", "weightWords("]) assert.ok(card.includes(fn), `the card reads ${fn}`);
  for (const [file, text] of [["the editor", editor], ["the card", card]]) {
    assert.ok(!/=== 1 \? "failure"/.test(text) && !text.includes("cooling · retry in"), `${file} says a badge in the catalog's words`);
    assert.ok(!/\?\? "fallback"/.test(text), `${file} names no default strategy of its own`);
  }
  assert.ok(!editor.includes("function shortDuration"), "a span is the formatter's to say");
  assert.ok(!card.includes(">weight {"), "a weight is the catalog's to say");
});
