/**
 * The New Goal dialog's decisions, tested where they live. Run with
 * `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import { attachRefusedWords, canSubmit, captureLabel, captureToast, documentsHint, goalBody, staffHint, staffNotOffered } from "./newGoalModel.mjs";

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
  assert.deepEqual(goalBody({ statement: "Ship", mode: "auto", assignees: ["team:01TEAM", "agent:developer"], tags: [], documents: [] }).assignees, ["team:01TEAM", "agent:developer"]);
  assert.equal("assignees" in goalBody({ statement: "Ship", mode: "auto", assignees: [], tags: [], documents: [] }), false, "nobody is absence on the wire");
  // Who carries it is the person's own pick in the dialog — agents and teams — and no team rides in from the Teams screen.
  const dialog = readFileSync(new URL("./NewGoalDialog.tsx", import.meta.url), "utf8");
  assert.ok(dialog.includes("assignees: staff,") && !dialog.includes("pendingTeam"), "the dialog's own pick, and nothing handed over");
  assert.ok(dialog.includes('const STAFF_KINDS: AssigneeKind[] = ["agent", "team"];'), "agents and teams carry steps; people decide gates elsewhere");
  assert.ok(dialog.includes("exclude={notOffered}") && dialog.includes("staffNotOffered(ws.agents)"), "the core and the disabled agents are never offered");
  assert.ok(dialog.includes("setStaff([]);") && dialog.includes("setStaffOpen(false);"), "a closed or captured dialog forgets the pick");
});

test("who carries it: never the platform's own agents or a disabled one, and the hint says what nobody picked means", () => {
  const agents = [
    { id: "developer", origin: "local", enabled: true },
    { id: "general-agent", origin: "core", enabled: true },
    { id: "workflow-agent", origin: "core", enabled: true },
    { id: "retired", origin: { catalog: { slug: "retired" } }, enabled: false },
    { id: "reviewer", origin: { catalog: { slug: "code-reviewer" } }, enabled: true },
  ];
  assert.deepEqual(staffNotOffered(agents), ["general-agent", "workflow-agent", "retired"]);
  assert.deepEqual(staffNotOffered([]), []);
  assert.deepEqual(staffNotOffered(undefined), []);
  assert.match(staffHint(0), /^Optional\. With nobody picked, the Workflow Agent staffs every step from the agents and teams that are enabled\.$/);
  assert.match(staffHint(2), /from these alone/);
  assert.equal(staffHint(1), staffHint(5), "one sentence for any pick");
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
