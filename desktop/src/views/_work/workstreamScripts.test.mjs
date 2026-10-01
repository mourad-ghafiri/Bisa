import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { ENV_VARS, MAX_SCRIPT_CHARS, PHASES, TIMEOUT_KEY, isDirty, needsApproval, scriptEdits, scriptRefusal, trustLine, validateScript } from "./workstreamScripts.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SETTINGS = readFileSync(join(HERE, "../../../../crates/bisa-core/src/settings.rs"), "utf8");
const SCRIPTS_RS = readFileSync(join(HERE, "../../../../crates/bisa-engine/src/scripts.rs"), "utf8");

const view = {
  timeout_secs: 300,
  scripts: [
    { phase: "pre_create", command: "", trusted: true },
    { phase: "post_create", command: "npm install", trusted: true },
    { phase: "clean", command: "docker compose down", trusted: false },
  ],
};

test("the three phases name registered settings, in the order a checkout meets them", () => {
  assert.deepEqual(PHASES.map((p) => p.id), ["pre_create", "post_create", "clean", "run"], "the three lifecycle scripts, then the run command the IDE's Terminal menu opens");
  assert.match(PHASES[3].when, /Browser menu/);
  for (const p of PHASES) {
    assert.ok(SETTINGS.includes(`"${p.key}"`), `${p.key} is in the registry`);
    assert.ok(SCRIPTS_RS.includes(`"${p.key}"`), `${p.key} is what the engine reads`);
    assert.ok(p.when.length > 40 && p.cwd, `${p.id} says when and where it runs`);
  }
  assert.ok(SETTINGS.includes(`"${TIMEOUT_KEY}"`));
});

test("every variable the card documents is one the engine sets", () => {
  for (const [name] of ENV_VARS) {
    assert.ok(name.startsWith("BISA_"), `${name} wears the engine's prefix`);
    assert.ok(SCRIPTS_RS.includes(`"${name}"`), `${name} is set by scripts.rs`);
  }
});

test("edits seed from the view, know when they differ, and become one settings write", () => {
  const current = scriptEdits(view);
  assert.deepEqual(current, { texts: { pre_create: "", post_create: "npm install", clean: "docker compose down", run: "" }, timeout: 300 });
  assert.deepEqual(scriptEdits(null), { texts: { pre_create: "", post_create: "", clean: "", run: "" }, timeout: 300 }, "no view yet: empty, the default timeout");
  assert.equal(isDirty(current, scriptEdits(view)), false);
  const edited = { ...current, texts: { ...current.texts, pre_create: "  cp .env.example .env  " } };
  assert.equal(isDirty(current, edited), true);
  assert.equal(isDirty(current, { ...current, timeout: 60 }), true);
});

test("a script has one rule — a bound — and newlines are fine", () => {
  assert.equal(validateScript("npm ci\nnpm run build"), null);
  assert.equal(validateScript(""), null);
  assert.match(validateScript("x".repeat(MAX_SCRIPT_CHARS + 1)), /At most/);
});

test("the trust line reads the node's answer and Approve appears only when something is unapproved", () => {
  assert.equal(trustLine(view.scripts[0]), null, "no script, no line");
  assert.equal(trustLine(view.scripts[1]).tone, "ok");
  assert.equal(trustLine(view.scripts[2]).tone, "warn");
  assert.match(trustLine(view.scripts[2]).text, /will not run/);
  assert.equal(needsApproval(view), true);
  assert.equal(needsApproval({ ...view, scripts: view.scripts.map((s) => ({ ...s, trusted: true })) }), false);
  assert.equal(needsApproval(null), false);
});

test("a script_failed refusal names the phase and carries the tail", () => {
  const r = scriptRefusal({ message: "the pre-create script exited with status 3", detail: { phase: "pre_create", output: "nope" } });
  assert.equal(r.title, "The pre-create script refused");
  assert.equal(r.output, "nope");
  assert.equal(scriptRefusal({ message: "x", detail: null }).title, "A workstream script refused");
  assert.equal(scriptRefusal({ message: "x" }).output, "");
});
