/**
 * Who is working under a heading of the project rail (ide/07).
 * Run with `node --test --import ./src/i18n/preload.mjs src/views/_workbench/railHarnessesModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";

import { MAX_HEADING_LINES, MAX_HEADING_MARKS, headingHarnessWords, headingMarks, standingHarnesses } from "./railHarnessesModel.mjs";

const T = (key, id, harness = "claude-code", status = "live", extra = {}) => ({ key, scope: "workstream", id, harness, label: null, resume: false, generation: 0, liveness: { status, code: status === "exited" ? 1 : 0 }, restoring: false, ...extra });
const S = (id, workstream, word = "running", extra = {}) => ({
  id,
  kind: "worker",
  state: word === "running" ? { state: "running", tool: "Edit", args: "" } : word === "failed" ? { state: "failed", reason: "boom" } : { state: word },
  since: 1,
  harness: "claude-code",
  work_item: null,
  workstream,
  project: null,
  goal: null,
  cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 },
  children: [],
  last_activity: 1,
  ...extra,
});
const LABELS = { "claude-code": "Claude Code", codex: "Codex" };

test("a harness is open while its session is live — reported from a terminal or an engine's — or while an unclaimed shell with a harness is live; nothing else is", () => {
  const sessions = [
    S("live", "w1", "running"),
    S("thinks", "w1", "thinking", { harness: "codex", kind: "terminal", children: [{ id: "a1", name: "explore", description: "map", state: { state: "thinking" }, since: 2 }] }),
    S("done", "w1", "done"),
    S("failed", "w1", "failed"),
    S("elsewhere", "w2", "running"),
    S("mid-open", "w1", "running", { kind: "terminal" }),
  ];
  const terminals = [
    { ...T("t-thinks", "w1", "codex"), sessionId: "thinks" },
    T("t-typed", "w1", null, "live", { running: "claude-code" }),
    T("t-shell", "w1", null, "live"),
    T("t-gone", "w1", "codex", "exited"),
  ];
  const standing = standingHarnesses(sessions, terminals, "w1");
  assert.deepEqual(
    standing.map((h) => [h.harness, h.sessionId, h.terminalKey, h.state?.state ?? null]),
    [
      ["claude-code", null, "t-typed", null],
      ["claude-code", "live", null, "running"],
      ["codex", "thinks", "t-thinks", "thinking"],
    ],
    "the rail's order: shells first, then sessions; a done or failed session, a plain shell, an exited shell and a terminal session no tab claims are not open; a sub-agent is its parent's; another workstream's is not here",
  );
  assert.deepEqual(standingHarnesses([], [], "w1"), []);
});

test("a heading wears one mark per distinct harness, first seen first, bounded", () => {
  const list = ["codex", "claude-code", "codex", "pi", "goose"].map((harness) => ({ harness }));
  assert.deepEqual(headingMarks(list), ["codex", "claude-code", "pi"]);
  assert.equal(headingMarks(list).length, MAX_HEADING_MARKS);
  assert.deepEqual(headingMarks([]), []);
});

test("the tip says how many are open and names each one's place, harness and doing, bounded with the rest counted", () => {
  const h = (project, workstream, harness, state, i) => ({ harness, state, sessionId: `s${i}`, terminalKey: null, projectId: `p${i}`, project, workstream });
  const one = headingHarnessWords([h("Shop", "app", "claude-code", { state: "running", tool: "Edit" }, 1)], LABELS);
  assert.equal(one.title, "1 harness open");
  assert.deepEqual(one.lines, [{ harness: "claude-code", text: "Shop › app — Claude Code · running Edit" }]);
  assert.equal(one.more, 0);
  const shell = headingHarnessWords([h("Shop", "site", "codex", null, 2)], LABELS);
  assert.equal(shell.lines[0].text, "Shop › site — Codex · in a shell", "a harness that does not report is alive and no more");
  const unknown = headingHarnessWords([h("Shop", "site", "mystery", { state: "idle" }, 3)], LABELS);
  assert.equal(unknown.lines[0].text, "Shop › site — mystery · idle", "a harness with no label goes by its id");
  const many = headingHarnessWords(Array.from({ length: 6 }, (_, i) => h(`P${i}`, "main", "claude-code", { state: "waiting", on: { on: "permission", tool: "Bash" } }, i)), LABELS);
  assert.equal(many.title, "6 harnesses open");
  assert.equal(many.lines.length, MAX_HEADING_LINES);
  assert.equal(many.more, 2);
  assert.match(many.lines[0].text, /waiting on you — permission: Bash$/);
});
