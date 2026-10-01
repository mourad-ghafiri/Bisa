import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { AFTER_MERGE, CLEANUP, STEP_ORDER, afterMergePlan, stepsToRun, summaryOf, toggleStep } from "./afterMergeModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SETTINGS = readFileSync(join(HERE, "../../../../crates/bisa-core/src/settings.rs"), "utf8");

function choiceWords(key) {
  const m = SETTINGS.match(new RegExp(`"${key.replace(".", "\\.")}",\\s*Choice\\(&\\[([^\\]]+)\\]\\)`));
  assert.ok(m, `${key} is a Choice in the registry`);
  return [...m[1].matchAll(/"([a-z_]+)"/g)].map((x) => x[1]);
}

const base = { cleanup: "ask", afterMerge: "ask", pullMode: "ff_only", primaryBusy: 0, defaultBranch: "main", branch: "work/dark-mode" };

test("the policy words are the registry's", () => {
  assert.deepEqual([...CLEANUP], choiceWords("workstreams.cleanup"));
  assert.deepEqual([...AFTER_MERGE], choiceWords("workstreams.after_merge"));
});

test("ask + ask: a dialog with every step checked, the pull before the leaving", () => {
  const { mode, steps } = afterMergePlan(base);
  assert.equal(mode, "dialog");
  assert.deepEqual(steps.map((s) => [s.id, s.checked, s.available]), [["pull", true, true], ["delete", true, true], ["return", true, true]]);
  assert.match(steps[0].label, /Pull main \(fast-forward\)/);
  assert.match(steps[1].label, /work\/dark-mode/);
  assert.deepEqual(stepsToRun(steps), ["pull", "delete", "return"]);
  assert.deepEqual([...STEP_ORDER], ["pull", "delete", "return"], "the pull happens on the primary before anything leaves");
});

test("a harness running on the primary keeps the return and the pull off, and says why", () => {
  const { mode, steps } = afterMergePlan({ ...base, primaryBusy: 2 });
  assert.equal(mode, "dialog");
  const pull = steps.find((s) => s.id === "pull");
  const ret = steps.find((s) => s.id === "return");
  const del = steps.find((s) => s.id === "delete");
  assert.equal(pull.available, false);
  assert.equal(pull.checked, false);
  assert.match(pull.reason, /2 sessions run on main/);
  assert.equal(ret.available, false);
  assert.equal(del.checked, true, "deleting the merged checkout is still fine");
  assert.deepEqual(stepsToRun(steps), ["delete"]);
  assert.deepEqual(stepsToRun(toggleStep(steps, "return")), ["delete"], "an unavailable step cannot be switched on");
  assert.equal(afterMergePlan({ ...base, primaryBusy: 1 }).steps[0].reason, "1 session runs on main — stop them first, or stay here.");
});

test("the automatic policies run silently — unless the primary is busy, which asks", () => {
  const auto = afterMergePlan({ ...base, cleanup: "remove_when_merged", afterMerge: "return_and_pull", pullMode: "rebase" });
  assert.equal(auto.mode, "silent");
  assert.deepEqual(stepsToRun(auto.steps), ["pull", "delete", "return"]);
  assert.match(auto.steps[0].label, /rebase/);
  const busy = afterMergePlan({ ...base, cleanup: "remove_when_merged", afterMerge: "return_and_pull", primaryBusy: 1 });
  assert.equal(busy.mode, "dialog", "never pull under a running session without asking");
  assert.deepEqual(stepsToRun(busy.steps), ["delete"]);
  const keepStay = afterMergePlan({ ...base, cleanup: "keep", afterMerge: "stay" });
  assert.equal(keepStay.mode, "none");
  assert.deepEqual(stepsToRun(keepStay.steps), []);
  const keepReturn = afterMergePlan({ ...base, cleanup: "keep", afterMerge: "return_and_pull" });
  assert.equal(keepReturn.mode, "silent");
  assert.deepEqual(stepsToRun(keepReturn.steps), ["pull", "return"]);
  const removeStay = afterMergePlan({ ...base, cleanup: "remove_when_merged", afterMerge: "stay" });
  assert.equal(removeStay.mode, "silent");
  assert.deepEqual(stepsToRun(removeStay.steps), ["delete"]);
  const askStay = afterMergePlan({ ...base, cleanup: "ask", afterMerge: "stay" });
  assert.equal(askStay.mode, "dialog");
  assert.deepEqual(stepsToRun(askStay.steps), ["delete"]);
});

test("a dirty checkout is said so on the delete step; toggles flip; the summary reads as a sentence", () => {
  const { steps } = afterMergePlan({ ...base, dirty: true });
  assert.match(steps[1].detail, /uncommitted changes/);
  const off = toggleStep(steps, "delete");
  assert.equal(off[1].checked, false);
  assert.deepEqual(stepsToRun(off), ["pull", "return"]);
  assert.equal(summaryOf({ pull: "done", delete: "done", return: "done" }, "main"), "Main pulled, the workstream removed, now on main.");
  assert.equal(summaryOf({ pull: "conflict" }, "main"), "The pull stopped on conflicts in main.");
  assert.equal(summaryOf({ delete: "failed" }, null), "The workstream kept (removing it failed).");
  assert.equal(summaryOf({}, "main"), "Nothing changed.");
});

test("deleting the checkout says what still stands in it before the person confirms, and the summary says what went with it", () => {
  const standing = { harnesses: 1, shells: 2, agents: 0 };
  const { steps } = afterMergePlan({ ...base, terminated: standing });
  assert.match(steps[1].detail, /Saved to Safety first\. What still stands in it ends: 1 harness terminated and 2 shells closed\.$/);
  assert.doesNotMatch(afterMergePlan(base).steps[1].detail, /stands in it/, "nothing standing there: nothing said");
  assert.doesNotMatch(steps[0].detail + steps[2].detail, /terminated/, "the pull and the return end nothing");
  assert.equal(summaryOf({ delete: "done" }, "main", standing), "The workstream removed (1 harness terminated and 2 shells closed).");
  assert.equal(summaryOf({ delete: "done", return: "done" }, "main", { harnesses: 0, shells: 0, agents: 1 }), "The workstream removed (1 agent session aborted), now on main.");
  assert.equal(summaryOf({ delete: "failed" }, "main", standing), "The workstream kept (removing it failed).", "a kept workstream terminated nothing the summary can claim");
});

test("a primary that could not be read is unknown, never idle: the pull waits with its own reason", async () => {
  const { afterMergePlan } = await import("./afterMergeModel.mjs");
  const plan = afterMergePlan({ cleanup: "delete", afterMerge: "return", pullMode: "ff_only", primaryBusy: null, defaultBranch: "main", branch: "topic" });
  const pull = plan.steps.find((s) => s.id === "pull");
  assert.equal(pull.available, false);
  assert.match(pull.reason, /could not be read/);
  assert.equal(pull.checked, false);
  const idle = afterMergePlan({ cleanup: "delete", afterMerge: "return", pullMode: "ff_only", primaryBusy: 0, defaultBranch: "main", branch: "topic" }).steps.find((s) => s.id === "pull");
  assert.equal(idle.available, true, "a read zero is idle");
});
