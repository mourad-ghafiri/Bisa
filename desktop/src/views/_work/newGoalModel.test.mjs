/**
 * The New Goal dialog's decisions, tested where they live. Run with
 * `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import { attachRefusedWords, canSubmit, captureLabel, captureToast, documentsHint, goalBody } from "./newGoalModel.mjs";

const brief = { sha256: "ab".repeat(32), name: "brief.pdf", mime: "application/pdf", size: 1234 };

const form = (over = {}) => ({
  statement: "  ship the checkout  ",
  mode: "auto",
  assignees: [],
  tags: [],
  documents: [],
  ...over,
});

test("a capture sends the statement and its mode, and only what was set beside them", () => {
  assert.deepEqual(goalBody(form()), { statement: "ship the checkout", mode: "auto" });
  assert.deepEqual(goalBody(form({ mode: "manual" })), { statement: "ship the checkout", mode: "manual" });
  assert.deepEqual(goalBody(form({ mode: "guided", documents: [brief], tags: ["web"], assignees: ["team:eng"] })), {
    statement: "ship the checkout",
    mode: "guided",
    assignees: ["team:eng"],
    tags: ["web"],
    documents: [brief],
  });
  // A document travels as its descriptor alone: what a row carries beside
  // it — a selection, a preview — is the row's, and the node refuses a key
  // it does not know.
  assert.deepEqual(goalBody(form({ documents: [{ ...brief, selected: true, preview: "blob:1" }] })).documents, [brief]);
  // No workflow and no inputs ever travel: the agent designs, or the person does.
  assert.equal("workflow" in goalBody(form()), false);
  assert.equal("inputs" in goalBody(form()), false);
});

test("capture waits for a statement, a request in flight and an upload still going", () => {
  assert.equal(canSubmit({ statement: "x", busy: false, uploading: false }), true);
  assert.equal(canSubmit({ statement: "   ", busy: false, uploading: false }), false);
  assert.equal(canSubmit({ statement: "x", busy: true, uploading: false }), false);
  assert.equal(canSubmit({ statement: "x", busy: false, uploading: true }), false, "a goal captured before its context arrived would run without it");
  assert.equal(captureLabel({ busy: false, uploading: true }), "Uploading…");
  assert.equal(captureLabel({ busy: true, uploading: true }), "Capturing…");
  assert.equal(captureLabel({ busy: false, uploading: false }), "Capture");
});

test("the words around the documents and after the capture", () => {
  assert.match(documentsHint(0), /^Optional/);
  assert.equal(documentsHint(1), "1 document goes with the goal.");
  assert.equal(documentsHint(3), "3 documents go with the goal.");
  assert.match(captureToast("auto"), /Workflow Agent/);
  assert.match(captureToast("guided"), /adopt/);
  assert.match(captureToast("manual"), /Workflow tab/);
});

test("assignees a body names travel in the wire's word, and nobody is absence on the wire", () => {
  assert.deepEqual(goalBody({ statement: "Ship", mode: "auto", assignees: ["team:01TEAM"], tags: [], documents: [] }).assignees, ["team:01TEAM"]);
  assert.equal("assignees" in goalBody({ statement: "Ship", mode: "auto", assignees: [], tags: [], documents: [] }), false, "nobody is absence on the wire");
  // The dialog hands a capture to nobody: the Teams screen's door to it is gone.
  const dialog = readFileSync(new URL("./NewGoalDialog.tsx", import.meta.url), "utf8");
  assert.ok(dialog.includes("assignees: [],") && !dialog.includes("pendingTeam"), "no team rides in from elsewhere");
});

test("projects that could not be attached are said once, counted, with the first reason — and the goal stands", () => {
  assert.equal(attachRefusedWords([]), null);
  assert.equal(attachRefusedWords(null), null);
  assert.equal(attachRefusedWords([{ project: "P1", reason: "the project is archived" }]), "The goal stands, but 1 project could not be attached to it: the project is archived");
  assert.equal(attachRefusedWords([{ project: "P1", reason: "the project is archived" }, { project: "P2", reason: "no such project" }]), "The goal stands, but 2 projects could not be attached to it: the project is archived");
  // The dialog tries each project on its own, logs each refusal, and goes on to the landing.
  const dialog = readFileSync(new URL("./NewGoalDialog.tsx", import.meta.url), "utf8");
  const loop = dialog.slice(dialog.indexOf("const refused"), dialog.indexOf("attachRefusedWords(refused)"));
  assert.ok(loop.includes("for (const pid of") && loop.includes("try {") && loop.includes("refused.push("), "one refusal never stops the ones after it");
  assert.ok(/log\.(warn|error)\(/.test(loop), "a refusal is the log's too");
});
