/**
 * One menu for a workstream row, wherever it is drawn. Run with `node --test desktop/src/views/_workbench/railMenuModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { agentRowMenuSpec, workstreamMenuSpec } from "./railMenuModel.mjs";

const ids = (ctx) => workstreamMenuSpec({ primary: false, exists: true, harnesses: [{ id: "claude-code", label: "Claude Code" }], rename: true, close: true, ...ctx }).map((i) => i.id);

test("a workstream row opens, renames, starts a shell or a harness, diffs and closes — no pull-request verb, since Open lands on the panel whose lifecycle it is; the primary has no branch verbs", () => {
  assert.deepEqual(ids({}), ["open", "rename", "new-shell", "harness:claude-code", "diff", "close"]);
  assert.ok(!ids({}).includes("pr"), "the pull request is the Workstreams panel's lifecycle, not a verb");
  assert.deepEqual(ids({ primary: true }), ["open", "rename", "new-shell", "harness:claude-code"], "nothing to merge the primary into, nothing to diff it against, no closing it");
  assert.deepEqual(ids({ rename: false, close: false }), ["open", "new-shell", "harness:claude-code", "diff"], "a surface without the dialogs offers no verb it cannot finish");
  const items = workstreamMenuSpec({ primary: false, exists: false, harnesses: [{ id: "omp", label: "OMP" }], rename: true, close: true });
  assert.ok(items.find((i) => i.id === "new-shell").disabled && items.find((i) => i.id === "harness:omp").disabled, "no checkout on disk: nothing to run in");
  assert.equal(items.find((i) => i.id === "harness:omp").label, "New OMP here");
  assert.equal(items.find((i) => i.id === "harness:omp").harness, "omp");
  assert.ok(items.find((i) => i.id === "close").danger);
  assert.ok(items.find((i) => i.id === "new-shell").separatorBefore && items.find((i) => i.id === "diff").separatorBefore, "a rule before the shells and one before the branch's own verbs");
  assert.ok(!items.find((i) => i.id === "close").separatorBefore, "closing sits with the diff, under one rule");
});

test("a session row's menu offers Abort only while the session runs or waits on you — never idle between turns; a harness in a terminal only Terminate; a sub-agent nothing", () => {
  const row = (state, extra = {}) => ({ parent: null, terminalKey: null, gateId: null, state, ...extra });
  const ids = (r) => agentRowMenuSpec(r).map((i) => i.id);
  assert.deepEqual(ids(row({ state: "running", tool: "Bash", args: "", tier: "exec" })), ["show", "abort"]);
  assert.deepEqual(ids(row("thinking")), ["show", "abort"]);
  assert.deepEqual(ids(row("idle")), ["show"], "alive between turns, nothing running to abort");
  assert.deepEqual(ids(row("done")), ["show"]);
  assert.deepEqual(ids(row("parked")), ["show"]);
  assert.deepEqual(ids(row({ state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } }, { gateId: "g1" })), ["show", "answer", "abort"]);
  const abort = agentRowMenuSpec(row("thinking")).find((i) => i.id === "abort");
  assert.ok(abort.danger && abort.separatorBefore, "Abort is red, under a rule");
  assert.deepEqual(ids(row("running", { terminalKey: "t1" })), ["terminate"], "a harness in a terminal is ended there");
  assert.deepEqual(ids(row("idle", { terminalKey: "t1" })), [], "an idle harness in its terminal has nothing to terminate from here — its tab closes from the strip");
  assert.deepEqual(ids(row("running", { parent: "p" })), [], "a sub-agent has no verbs of its own");
});
