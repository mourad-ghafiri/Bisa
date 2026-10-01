/**
 * Effort as the desktop knows it: the words are the core's, in the core's
 * order; a level is fitted as the core fits it; a picker offers what a model
 * takes and never drops a value already saved; the chain answers as the
 * core's `resolve` does. Run with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/views/_work/effortModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  DEFAULT_EFFORT,
  EFFORTS,
  EFFORT_CHOICES,
  EFFORT_SETTING,
  clampEffort,
  effortChoice,
  effortLabel,
  effortOptions,
  effortSetting,
  effortWords,
  effortsAcross,
  effortsFor,
  offersEffort,
  planEfforts,
  resolveEffort,
  withEffort,
} from "./effortModel.mjs";

const rust = (file) => readFileSync(new URL(`../../../../crates/bisa-core/src/${file}`, import.meta.url), "utf8");

/** The variants of one `pub enum` of `effort.rs`, one a line at four spaces, in order. */
function variants(name) {
  const m = rust("effort.rs").match(new RegExp(`pub enum ${name} \\{\\n([\\s\\S]*?)\\n\\}`));
  assert.ok(m, `${name} is declared in effort.rs`);
  return m[1].split("\n").map((line) => line.match(/^ {4}([A-Z][A-Za-z]*),$/)?.[1]);
}

/** The wire word of each level, read off `Effort::as_str`'s arms. */
function wireWords() {
  return new Map([...rust("effort.rs").matchAll(/Effort::([A-Z][a-z]+) => "([a-z]+)",/g)].map((m) => [m[1], m[2]]));
}

/** A `Choice` setting's words, read from the registry's source. */
function choicesOf(key) {
  const m = rust("settings.rs").match(new RegExp(`"${key.replaceAll(".", "\\.")}",\\s*Choice\\(&\\[([^\\]]*)\\]\\),\\s*json!\\("([a-z]+)"\\)`));
  assert.ok(m, `${key} is a registered choice`);
  return { words: [...m[1].matchAll(/"([a-z]+)"/g)].map((w) => w[1]), fallback: m[2] };
}

const CLAUDE_CODE = {
  harness: "claude-code",
  efforts: ["low", "medium", "high", "max"],
  models: [
    { id: "claude-opus-5-5[1m]", efforts: ["low", "medium", "high", "xhigh", "max"] },
    { id: "claude-sonnet-4-6", efforts: ["low", "medium", "high", "max"] },
    { id: "claude-haiku-4-5", efforts: [] },
  ],
};
const CODEX = { harness: "codex", efforts: ["low", "medium", "high"], models: [] };
const GOOSE = { harness: "goose", efforts: [], models: [] };

test("the levels and the choices are the core's, word for word and in its order", () => {
  const words = wireWords();
  const levels = variants("Effort");
  assert.ok(levels.every(Boolean), "one variant a line");
  assert.deepEqual(levels.map((v) => words.get(v)), [...EFFORTS]);
  assert.deepEqual([...EFFORTS], ["minimal", "low", "medium", "high", "xhigh", "max"]);
  const choices = variants("EffortChoice");
  assert.equal(choices[0], "Auto", "auto leads what may be asked");
  assert.deepEqual(choices.slice(1), levels, "then the levels, as they are");
  assert.deepEqual([...EFFORT_CHOICES], ["auto", ...EFFORTS]);
  assert.ok(EFFORTS.includes("xhigh") && !EFFORTS.includes("x_high"), "one word, no underscore: a harness reads xhigh");
  assert.match(rust("effort.rs"), /pub const DEFAULT: Effort = Effort::High;/);
  assert.equal(DEFAULT_EFFORT, "high");
  assert.ok(Object.isFrozen(EFFORTS) && Object.isFrozen(EFFORT_CHOICES));
});

test("the setting at the end of the chain is the registry's, with the choices and the default the core names", () => {
  assert.equal(EFFORT_SETTING, "agents.effort");
  const agents = choicesOf(EFFORT_SETTING);
  assert.deepEqual(agents.words, [...EFFORT_CHOICES]);
  assert.equal(agents.fallback, DEFAULT_EFFORT);
  // The judge's own session and the classifier's take a level, never `auto`.
  for (const key of ["decisions.harness.effort", "security.classifier.effort"]) {
    assert.deepEqual(choicesOf(key).words, [...EFFORTS], key);
    assert.equal(choicesOf(key).fallback, DEFAULT_EFFORT, key);
  }
});

test("a value is a choice or nothing, and every choice has a label a person reads", () => {
  for (const word of EFFORT_CHOICES) assert.equal(effortChoice(word), word);
  for (const odd of ["", "High", "HIGH", " high", "x_high", "off", "none", "ultra", null, undefined, 3, {}]) assert.equal(effortChoice(odd), null, String(odd));
  assert.deepEqual(EFFORT_CHOICES.map(effortLabel), ["Auto", "Minimal", "Low", "Medium", "High", "Extra high", "Max"]);
  assert.equal(effortLabel(null), "Inherit");
  assert.equal(effortLabel(undefined), "Inherit");
  assert.equal(effortLabel(""), "Inherit", "nothing chosen inherits");
  assert.equal(effortLabel("ultra"), "ultra", "a word this build does not know is shown as it is");
});

test("a level is fitted to itself, then the nearest below, then the lowest above — the core's table", () => {
  const five = ["low", "medium", "high", "xhigh", "max"];
  assert.equal(clampEffort("high", five), "high");
  const four = ["low", "medium", "high", "max"];
  assert.equal(clampEffort("xhigh", four), "high", "the nearest below");
  assert.equal(clampEffort("max", four), "max");
  assert.equal(clampEffort("minimal", four), "low", "nothing below: the lowest above");
  const top = ["max", "high"];
  assert.equal(clampEffort("low", top), "high");
  assert.equal(clampEffort("xhigh", top), "high");
  assert.equal(clampEffort("high", ["max", "low", "low", "medium"]), "medium", "neither the order nor a repeat matters");
  for (const level of EFFORTS) {
    assert.equal(clampEffort(level, []), null, "no control: nothing is sent");
    assert.equal(clampEffort(level, EFFORTS), level);
    assert.equal(clampEffort(level, ["medium"]), "medium");
  }
  assert.equal(clampEffort("auto", four), null, "auto is a choice and never a level");
  assert.equal(clampEffort("high", ["ultra", "off"]), null, "a word that is no level is not available");
});

test("what a harness takes for a model: the listed model's own, the harness's for an id typed by hand, nothing unasked", () => {
  assert.deepEqual(effortsFor(CLAUDE_CODE, "claude-opus-5-5[1m]"), ["low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(effortsFor(CLAUDE_CODE, "claude-sonnet-4-6"), ["low", "medium", "high", "max"]);
  assert.deepEqual(effortsFor(CLAUDE_CODE, "claude-haiku-4-5"), [], "a listed model with no control has none, whatever the harness takes elsewhere");
  assert.deepEqual(effortsFor(CLAUDE_CODE, "some-model-typed-by-hand"), ["low", "medium", "high", "max"], "free text: the harness's own list");
  assert.deepEqual(effortsFor(CLAUDE_CODE, "claude-opus-5-5"), ["low", "medium", "high", "max"], "an id is matched exactly, as a pin is");
  assert.deepEqual(effortsFor(CLAUDE_CODE, null), ["low", "medium", "high", "max"], "no model: the harness's default model");
  assert.deepEqual(effortsFor(CODEX, "gpt-5.5-codex"), ["low", "medium", "high"]);
  assert.deepEqual(effortsFor(GOOSE, "anything"), []);
  assert.deepEqual(effortsFor(null, "claude-opus-5-5[1m]"), [], "a harness that has not answered");
  assert.deepEqual(effortsFor(undefined, null), []);
  assert.deepEqual(effortsFor({ models: [{ id: "m" }] }, "m"), [], "a field that is missing is no level");
  assert.deepEqual(effortsFor({ efforts: ["max", "ultra", "low", "low"], models: [] }, null), ["low", "max"], "lowest first, each once, levels only");
});

test("a plan's levels are what any of its enabled models takes; several harnesses, what any of them takes", () => {
  const plan = { models: [{ model: "claude-sonnet-4-6", enabled: true }, { model: "claude-opus-5-5[1m]" }, { model: "claude-haiku-4-5", enabled: false }] };
  assert.deepEqual(planEfforts(CLAUDE_CODE, plan), ["low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(planEfforts(CLAUDE_CODE, { models: [{ model: "claude-sonnet-4-6" }] }), ["low", "medium", "high", "max"]);
  assert.deepEqual(planEfforts(CLAUDE_CODE, { models: [{ model: "claude-haiku-4-5" }] }), []);
  assert.deepEqual(planEfforts(CLAUDE_CODE, { models: [] }), ["low", "medium", "high", "max"], "an empty plan runs the harness's own model");
  assert.deepEqual(planEfforts(CLAUDE_CODE, { models: [{ model: "claude-opus-5-5[1m]", enabled: false }] }), ["low", "medium", "high", "max"], "a plan with nothing enabled is an empty plan");
  assert.deepEqual(planEfforts(null, plan), []);
  assert.deepEqual(effortsAcross([CODEX, CLAUDE_CODE], "claude-opus-5-5[1m]"), ["low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(effortsAcross([CODEX, GOOSE], null), ["low", "medium", "high"]);
  assert.deepEqual(effortsAcross([GOOSE, null], null), []);
  assert.deepEqual(effortsAcross([], null), []);
  assert.deepEqual(effortsAcross(null, null), []);
});

const values = (options) => options.map((o) => o.value);

test("a picker offers Inherit and Auto when asked for, then only the levels the model takes", () => {
  const opus = effortOptions(effortsFor(CLAUDE_CODE, "claude-opus-5-5[1m]"), { inherit: true, auto: true });
  assert.deepEqual(values(opus), ["", "auto", "low", "medium", "high", "xhigh", "max"]);
  assert.deepEqual(opus.map((o) => o.label), ["Inherit", "Auto", "Low", "Medium", "High", "Extra high", "Max"]);
  assert.ok(offersEffort(opus));
  assert.deepEqual(values(effortOptions(["low", "medium", "high"], { auto: true })), ["auto", "low", "medium", "high"], "no Inherit unless asked for");
  assert.deepEqual(values(effortOptions(["low", "medium", "high"], { inherit: true })), ["", "low", "medium", "high"], "no Auto unless asked for");
  assert.deepEqual(values(effortOptions(["max", "high"])), ["high", "max"], "lowest first, whatever the order listed");
  assert.deepEqual(values(effortOptions(["medium"], { inherit: true, auto: true })), ["", "medium"], "one level is nothing for Auto to choose between");
});

test("a harness that has not answered is unknown, not none; one that answered with nothing offers nothing", () => {
  const unknown = effortOptions([], { inherit: true, auto: true, known: false });
  assert.deepEqual(values(unknown), ["", "auto", ...EFFORTS], "all six");
  assert.ok(offersEffort(unknown));
  const none = effortOptions(effortsFor(GOOSE, null), { inherit: true, auto: true, known: true });
  assert.deepEqual(values(none), [""]);
  assert.equal(offersEffort(none), false, "Inherit alone is no choice: there is nothing to pick");
  assert.deepEqual(effortOptions([]), [], "known is the default: a list is what a harness said");
  assert.equal(offersEffort([]), false);
  assert.equal(offersEffort(null), false);
  assert.deepEqual(values(effortOptions(["low", "high"], { known: false })), ["low", "high"], "a list that is there is what is offered");
});

test("a value already saved stays in the picker when the model no longer offers it — shown, and marked as fitted at launch", () => {
  const sonnet = effortsFor(CLAUDE_CODE, "claude-sonnet-4-6");
  const kept = effortOptions(sonnet, { inherit: true, auto: true, current: "xhigh" });
  assert.deepEqual(values(kept), ["", "auto", "low", "medium", "high", "max", "xhigh"]);
  assert.deepEqual(kept.at(-1), { value: "xhigh", label: "Extra high — fitted to High at launch", kept: true });
  const above = effortOptions(sonnet, { current: "minimal" }).at(-1);
  assert.deepEqual(above, { value: "minimal", label: "Minimal — fitted to Low at launch", kept: true }, "nothing below: the lowest above");
  // Offered still: it is its own option, once, unmarked.
  const offered = effortOptions(sonnet, { inherit: true, auto: true, current: "max" });
  assert.deepEqual(values(offered), ["", "auto", "low", "medium", "high", "max"]);
  assert.ok(offered.every((o) => !o.kept));
  // Where no level is listed the value is still there to be seen, and cleared.
  const none = effortOptions([], { inherit: true, auto: true, current: "high" });
  assert.deepEqual(none, [
    { value: "", label: "Inherit" },
    { value: "high", label: "High — not offered here", kept: true },
  ]);
  assert.ok(offersEffort(none), "a saved value is something to see and clear");
  // Auto saved where one level leaves nothing to choose between.
  assert.deepEqual(effortOptions(["medium"], { auto: true, current: "auto" }), [
    { value: "medium", label: "Medium" },
    { value: "auto", label: "Auto — not offered here", kept: true },
  ]);
  // Nothing saved, or a word that is no choice, adds nothing.
  for (const current of [null, undefined, "", "ultra"]) assert.deepEqual(values(effortOptions(sonnet, { current })), ["low", "medium", "high", "max"]);
});

test("the first that is set decides: the step, the model, the plan, the setting", () => {
  assert.deepEqual(resolveEffort(null, null, null, "high"), { kind: "level", level: "high", from: "setting" });
  assert.deepEqual(resolveEffort(null, null, "max", "high"), { kind: "level", level: "max", from: "plan" });
  assert.deepEqual(resolveEffort(null, "low", "max", "high"), { kind: "level", level: "low", from: "model" });
  assert.deepEqual(resolveEffort("xhigh", "low", "max", "high"), { kind: "level", level: "xhigh", from: "step" });
  assert.deepEqual(resolveEffort("medium", "auto", "auto", "auto"), { kind: "level", level: "medium", from: "step" }, "a level above an auto below it: the level, and nobody is asked");
  assert.deepEqual(resolveEffort(undefined, "", "ultra", "low"), { kind: "level", level: "low", from: "setting" }, "what is no choice is not set");
  assert.deepEqual(resolveEffort(null, null, null, undefined), { kind: "level", level: "high", from: "setting" }, "a setting not read yet is the default");
});

test("auto falls back to the next level down the chain, and to the default when there is none", () => {
  assert.deepEqual(resolveEffort(null, null, null, "auto"), { kind: "auto", fallback: "high", from: "setting", fallbackFrom: "default" });
  assert.deepEqual(resolveEffort(null, null, "auto", "low"), { kind: "auto", fallback: "low", from: "plan", fallbackFrom: "setting" });
  assert.deepEqual(resolveEffort("auto", "max", "low", "high"), { kind: "auto", fallback: "max", from: "step", fallbackFrom: "model" });
  assert.deepEqual(resolveEffort("auto", "auto", "medium", "high"), { kind: "auto", fallback: "medium", from: "step", fallbackFrom: "plan" }, "an auto further down is passed over");
  assert.deepEqual(resolveEffort("auto", "auto", "auto", "auto"), { kind: "auto", fallback: "high", from: "step", fallbackFrom: "default" });
  assert.deepEqual(resolveEffort(null, "auto", null, "minimal"), { kind: "auto", fallback: "minimal", from: "model", fallbackFrom: "setting" }, "what is above the auto is never its fallback");
});

test("the setting is read with the layer that holds it, and a value that is no choice is the default", () => {
  assert.deepEqual(effortSetting([{ key: "agents.effort", value: "max", origin: "workspace" }]), { setting: "max", origin: "workspace" });
  assert.deepEqual(effortSetting([{ key: "agents.default", value: "x", origin: "project" }, { key: "agents.effort", value: "auto", origin: "project" }]), { setting: "auto", origin: "project" });
  assert.deepEqual(effortSetting([{ key: "agents.effort", value: "high", origin: "default" }]), { setting: "high", origin: "default" });
  assert.deepEqual(effortSetting([{ key: "agents.effort", value: "ultra", origin: "workspace" }]), { setting: "high", origin: "default" });
  assert.deepEqual(effortSetting([]), { setting: "high", origin: "default" });
  assert.deepEqual(effortSetting(null), { setting: "high", origin: "default" });
});

test("the sentence says what runs and who decided, fitted to what the model takes", () => {
  const opus = { available: effortsFor(CLAUDE_CODE, "claude-opus-5-5[1m]"), known: true, origin: "workspace" };
  const sonnet = { available: effortsFor(CLAUDE_CODE, "claude-sonnet-4-6"), known: true, origin: "workspace" };
  assert.deepEqual(effortWords(resolveEffort(null, null, null, "high"), opus), { runs: "high", label: "High", hint: "Runs at High — the workspace's setting." });
  assert.equal(effortWords(resolveEffort(null, null, null, "high"), { ...opus, origin: "project" }).hint, "Runs at High — the project's setting.");
  assert.equal(effortWords(resolveEffort(null, null, null, "high"), { ...opus, origin: "default" }).hint, "Runs at High — the default.");
  assert.equal(effortWords(resolveEffort(null, null, "max", "high"), opus).hint, "Runs at Max — the agent's.");
  assert.equal(effortWords(resolveEffort(null, "low", "max", "high"), opus).hint, "Runs at Low — this model's own.");
  assert.equal(effortWords(resolveEffort("xhigh", "low", "max", "high"), opus).hint, "Runs at Extra high — the step's pin.");
  assert.deepEqual(effortWords(resolveEffort("xhigh", null, null, "high"), sonnet), {
    runs: "high",
    label: "High",
    hint: "Runs at High, the nearest to Extra high that is taken — the step's pin.",
  });
  assert.equal(effortWords(resolveEffort(null, null, null, "minimal"), sonnet).hint, "Runs at Low, the nearest to Minimal that is taken — the workspace's setting.");
});

test("auto says who names the level and what runs while nobody does; one level is no question", () => {
  const opus = { available: effortsFor(CLAUDE_CODE, "claude-opus-5-5[1m]"), known: true };
  assert.deepEqual(effortWords(resolveEffort(null, null, "auto", "max"), opus), {
    runs: "max",
    label: "Auto",
    hint: "The Decision-Making Agent names the level for each task; Max runs while it is off, unsure or does not answer.",
  });
  assert.equal(effortWords(resolveEffort("auto", null, null, "minimal"), opus).runs, "low", "the fallback is fitted too");
  assert.deepEqual(effortWords(resolveEffort(null, "auto", null, "high"), { available: ["medium"], known: true }), {
    runs: "medium",
    label: "Medium",
    hint: "One level to choose from, so nobody is asked: Medium runs.",
  });
});

test("a harness that has not answered is asked for the level as it is; where none is listed the sentence says so", () => {
  assert.deepEqual(effortWords(resolveEffort(null, null, "xhigh", "high"), { available: [], known: false }), { runs: "xhigh", label: "Extra high", hint: "Runs at Extra high — the agent's." });
  assert.deepEqual(effortWords(resolveEffort(null, null, "xhigh", "high")), { runs: "xhigh", label: "Extra high", hint: "Runs at Extra high — the agent's." }, "no facts: nothing is known");
  assert.deepEqual(effortWords(resolveEffort(null, null, "xhigh", "high"), { available: effortsFor(GOOSE, null), known: true }), {
    runs: null,
    label: "",
    hint: "No effort levels to choose from here.",
  });
  assert.equal(effortWords(resolveEffort(null, null, "auto", "high"), { available: [], known: true }).runs, null);
  // A listed model that takes none, on a harness whose other models do.
  assert.equal(effortWords(resolveEffort(null, null, "max", "high"), { available: effortsFor(CLAUDE_CODE, "claude-haiku-4-5"), known: true }).hint, "No effort levels to choose from here.");
});

test("an effort is set on its holder, and nothing chosen leaves no field — never an empty string", () => {
  const plan = { strategy: "fallback", models: [{ model: "claude-opus-5-5[1m]", weight: 1, enabled: true }] };
  const asked = withEffort(plan, "max");
  assert.deepEqual(asked, { ...plan, effort: "max" });
  assert.notEqual(asked, plan, "a new object");
  assert.deepEqual(plan, { strategy: "fallback", models: [{ model: "claude-opus-5-5[1m]", weight: 1, enabled: true }] }, "the one handed in is untouched");
  assert.deepEqual(withEffort(asked, "auto"), { ...plan, effort: "auto" });
  for (const nothing of [null, undefined, "", "ultra"]) {
    const cleared = withEffort(asked, nothing);
    assert.deepEqual(cleared, plan);
    assert.ok(!("effort" in cleared), "absent, so it inherits");
  }
  assert.deepEqual(withEffort({ kind: "agent", model: "m" }, "xhigh"), { kind: "agent", model: "m", effort: "xhigh" });
});
