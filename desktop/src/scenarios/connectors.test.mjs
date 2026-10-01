/**
 * Connectors as the desktop shows them (guide/connectors.md): a custom
 * definition is written only after the node has read it, the connector
 * step's spellings are its model's, a write is gated or owned by the switch,
 * and an unknown check state is said. Source assertions, as
 * `conversations.test.mjs` makes them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/connectors.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("a custom definition is validated on the node before it is written, and the rules are said", () => {
  const panel = src("../views/_settings/ConnectorsPanel.tsx");
  const save = panel.slice(panel.indexOf("const save = async"), panel.indexOf("const problems = problemLines(validation)"));
  assert.ok(save.indexOf("api.validateConnector(def)") < save.indexOf("api.createConnector(def)"), "Save validates first");
  assert.ok(save.includes("found.length > 0") && save.includes("return;"), "and writes nothing while a problem stands");
  assert.ok(save.includes("saveRefusedWords("), "saying so in the model's words");
  // The rules paragraph is one message of the catalog, said through `rich()`.
  assert.ok(panel.includes('rich("settings-connectors-panel-shape-blurb"'), "the panel says the rules paragraph");
  const blurb = readFileSync(new URL("../../../locales/en/desktop/settings.ftl", import.meta.url), "utf8");
  for (const rule of ["timeout_secs", "Idempotency-Key", "<code>page</code>", "Transfer-Encoding", "*.suffix", "64 KiB"]) {
    assert.ok(blurb.includes(rule), `the rules paragraph names ${rule}`);
  }
  assert.ok(panel.includes('aria-live="polite"'), "the paste-code box says when it goes away on its own");
});

test("the connector step's spellings and words are its model's, and a write is gated or owned", () => {
  const form = src("../views/_workflow/forms/ConnectorStepForm.tsx");
  assert.ok(form.includes('from "./connectorStepModel.mjs"'));
  assert.ok(!form.includes('const FIXED = "fixed:"'), "no spelling of its own");
  assert.ok(form.includes("<Switch") && form.includes("set({ unattended: on })"), "the person's word is a switch");
  assert.ok(form.includes("writeWords(unattended)"), "the warning reads by the switch");
  assert.ok(src("../views/_workflow/stepKinds.mjs").includes("ungated_write:"), "the validator's word has its sentence");
});

test("an unknown check state is said, never read as nothing", () => {
  const model = src("../views/_settings/connectorsModel.mjs");
  const tail = model.slice(model.indexOf("export function checkLine"));
  assert.ok(tail.includes("does not know"), "the default arm names the state");
});
