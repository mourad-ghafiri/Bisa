/**
 * The footer's terminals and harnesses are the rail's rows. Run with
 * `node --test desktop/src/shell/footerSessionsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { emptyPlaceIndex, footerSessions, isOpen, placeIndex, placeWords } from "./footerSessionsModel.mjs";
import { livenessWord } from "./terminalsModel.mjs";

const tab = (key, over = {}) => ({ key, scope: "workstream", id: "w1", harness: null, liveness: { status: "live" }, sessionId: null, ...over });
const session = (id, over = {}) => ({ id, kind: "worker", state: { state: "running" }, harness: "claude-code", agent: "general-agent", workstream: "w1", ...over });

test("a tab's word and whether it still stands", () => {
  assert.equal(livenessWord({ status: "live" }), "open");
  assert.equal(livenessWord({ status: "exited", code: 0 }), "exited");
  assert.equal(livenessWord({ status: "exited", code: 130 }), "exited (130)");
  assert.equal(livenessWord({ status: "unverifiable", reason: "restored" }), "unverifiable");
  assert.equal(isOpen({ status: "live" }), true);
  assert.equal(isOpen({ status: "unverifiable", reason: "restored" }), true, "contact lost is not death");
  assert.equal(isOpen({ status: "exited", code: 0 }), false);
});

test("a reported harness in a terminal is one harness row that remembers its tab, and no terminal row", () => {
  const terminals = [tab("t1", { harness: "claude-code", sessionId: "s1" }), tab("t2")];
  const sessions = [session("s1", { kind: "terminal" })];
  const out = footerSessions(terminals, sessions);
  assert.deepEqual(
    out.terminals.map((t) => t.key),
    ["t2"],
    "the claimed tab is the harness's row, not a terminal",
  );
  assert.deepEqual(
    out.harnesses.map((h) => [h.id, h.terminalKey]),
    [["s1", "t1"]],
  );
  assert.equal(out.openTerminals, 1);
});

test("an interactive session no tab claims is nowhere; an engine session with no tab is a harness row without a key", () => {
  const out = footerSessions([tab("t1")], [session("orphan", { kind: "terminal" }), session("s2", { kind: "worker" })]);
  assert.deepEqual(
    out.harnesses.map((h) => [h.id, h.terminalKey]),
    [["s2", null]],
  );
});

test("an ended session is not a harness row", () => {
  const out = footerSessions([], [session("s1", { state: { state: "done" } }), session("s2", { state: { state: "waiting" } })]);
  assert.deepEqual(
    out.harnesses.map((h) => h.id),
    ["s2"],
  );
});

test("open tabs count — live and unverifiable — exited ones are listed last and not counted", () => {
  const out = footerSessions(
    [tab("t1", { liveness: { status: "exited", code: 1 } }), tab("t2", { liveness: { status: "unverifiable", reason: "restored" } }), tab("t3", { scope: "goal", id: "g1" })],
    [],
  );
  assert.deepEqual(
    out.terminals.map((t) => [t.key, t.word, t.open]),
    [
      ["t3", "open", true],
      ["t2", "unverifiable", true],
      ["t1", "exited (1)", false],
    ],
  );
  assert.equal(out.openTerminals, 2);
  assert.equal(out.terminals[0].scope, "goal", "a tab keeps its place, whatever the scope");
  assert.deepEqual(footerSessions(null, null), { terminals: [], harnesses: [], openTerminals: 0 });
});

const ref = (id, project, kind, over = {}) => ({ project_name: "Bisa", path: `/ws/${id}`, exists: true, workstream: { id, project, name: null, kind, ...over } });
const WS = {
  projects: [{ project: { id: "p1", name: "Bisa" } }, { project: { id: "p2", name: "Shop" } }],
  workstreams: [
    ref("p1", "p1", { kind: "primary" }),
    ref("w1", "p1", { kind: "worktree", branch: "feat/a", base: "main" }),
    ref("w2", "p1", { kind: "worktree", branch: "feat/b", base: "main" }, { name: "Cart total" }),
    { ...ref("w3", "p2", { kind: "copy" }), project_name: null },
  ],
  goals: [{ id: "g1", label: "Ship the cart" }],
};

test("where a session stands: the project and the workstream, compactly, by the cards' own rule", () => {
  const index = placeIndex(WS);
  assert.equal(placeWords("workstream", "w1", index), "Bisa › feat/a", "a worktree by its branch");
  assert.equal(placeWords("workstream", "p1", index), "Bisa › primary", "the primary by its word");
  assert.equal(placeWords("workstream", "w2", index), "Bisa › Cart total", "a named workstream by its name");
  assert.equal(placeWords("workstream", "w3", index), "Shop › copy · w3", "a copy by its tail; the project by id when the ref names none");
  assert.equal(placeWords("project", "p2", index), "Shop");
  assert.equal(placeWords("goal", "g1", index), "Ship the cart");
  assert.equal(placeWords("work_item", "item-abcdef", index), "work item ·abcdef");
  assert.equal(placeWords("run", "01JRUN0000UVWXYZ", index), "run ·UVWXYZ", "a run of the workspace by its tail");
  assert.equal(placeWords("machine", "home", index), "this machine");
  assert.equal(placeWords("workstream", "gone-123456", index), "workstream ·123456", "an id the index never saw");
  assert.equal(placeWords("goal", "g9", emptyPlaceIndex()), "goal ·g9");
});

test("every footer row carries its place: a terminal where it is rooted, a harness its workstream's, else its project's, else nowhere", () => {
  const index = placeIndex(WS);
  const out = footerSessions([tab("t1", { id: "w1" }), tab("t2", { scope: "machine", id: "home" })], [session("s1", { workstream: "w2" }), session("s2", { workstream: null, project: "p2" }), session("s3", { workstream: null })], index);
  assert.deepEqual(
    out.terminals.map((t) => [t.key, t.place]),
    [
      ["t1", "Bisa › feat/a"],
      ["t2", "this machine"],
    ],
  );
  assert.deepEqual(
    out.harnesses.map((h) => [h.id, h.place]),
    [
      ["s1", "Bisa › Cart total"],
      ["s2", "Shop"],
      ["s3", "not in a checkout"],
    ],
  );
  assert.equal(footerSessions([tab("t1")], []).terminals[0].place, "workstream ·w1", "no index: the kind and the tail");
});

test("the index knows a project by slug, a workstream's project id and a goal's workflow", () => {
  const index = placeIndex({
    projects: [{ project: { id: "p1", slug: "shop", name: "Shop" } }],
    workstreams: [{ workstream: { id: "w1", project: "p1", kind: "primary", name: "main" }, project_name: "Shop" }],
    goals: [{ id: "g1", label: "Ship the cart", workflow: "wf1", workflowName: "Ship it" }, { id: "g2", label: "Plain" }],
  });
  assert.deepEqual(index.projectSlugs.get("shop"), { id: "p1", name: "Shop" });
  assert.equal(index.workstreams.get("w1").projectId, "p1");
  assert.deepEqual(index.goals.get("g1"), { label: "Ship the cart", workflow: "wf1", workflowName: "Ship it" });
  assert.deepEqual(index.goals.get("g2"), { label: "Plain", workflow: null, workflowName: null });
  assert.equal(placeWords("goal", "g1", index), "Ship the cart");
  assert.equal(placeWords("goal", "g404", index), "goal ·g404");
  assert.equal(emptyPlaceIndex().projectSlugs.size, 0);
});
