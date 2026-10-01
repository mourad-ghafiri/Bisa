/**
 * The rules behind the git panel store. Run with
 * `node --test desktop/src/views/_work/gitPanelModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EMPTY_SESSION, MAX_SESSIONS, MAX_SESSION_DRAFTS, applied, draftStorageKey, failureOf, fileDraftKey, fingerprint, gitViewOf, parseGitView, remember, sameGitView, withGitView } from "./gitPanelModel.mjs";

test("the view part is kept and the rest starts clean", () => {
  const stood = {
    ...EMPTY_SESSION,
    busy: "push",
    note: "suggested from the diff",
    failure: failureOf("commit", "nothing to commit"),
    pending: { kind: "amend" },
    pullBanner: { kind: "pulled" },
    publish: { kind: "pushed" },
    operation: { kind: "rebase" },
    stale: 4,
    selection: { path: "src/a.ts", staged: true },
    changesFolds: ["section:staged", "dir:src"],
    graphRefs: "head",
  };
  const view = gitViewOf(stood);
  assert.deepEqual(view, { selection: { path: "src/a.ts", staged: true }, changesFolds: ["section:staged", "dir:src"], graphRefs: "head" });
  assert.deepEqual(Object.keys(view).sort(), ["changesFolds", "graphRefs", "selection"], "nothing of an operation is in it");
  // After a restart: read back as JSON, laid onto a clean session.
  const back = withGitView(EMPTY_SESSION, JSON.parse(JSON.stringify(view)));
  assert.deepEqual(gitViewOf(back), view);
  for (const clean of ["busy", "note", "failure", "pending", "pullBanner", "operation", "files", "stashing", "shownStash", "afterMerge"]) assert.equal(back[clean], null, `${clean} starts clean`);
  assert.deepEqual([back.publish, back.prPublish, back.prOpen, back.amend, back.stale], [{ kind: "none" }, { kind: "none" }, false, false, 0]);
  assert.deepEqual(gitViewOf(EMPTY_SESSION), { selection: null, changesFolds: [], graphRefs: "all" }, "a fresh panel's view is the beginning");
});

test("what is no view reads back as nothing", () => {
  for (const raw of [null, undefined, "", "head", 3, true, [], ["dir:src"]]) {
    assert.equal(parseGitView(raw), null, JSON.stringify(raw) ?? String(raw));
    assert.equal(withGitView(EMPTY_SESSION, raw), EMPTY_SESSION, "the session is left as it is");
  }
  assert.deepEqual(parseGitView({}), { selection: null, changesFolds: [], graphRefs: "all" }, "a record with no part is the clean view");
  assert.deepEqual(parseGitView({ selection: "src/a.ts", changesFolds: "dir:src", graphRefs: "every" }), { selection: null, changesFolds: [], graphRefs: "all" }, "a part that is not one is the clean session's");
  assert.deepEqual(parseGitView({ selection: { path: "", staged: true } }).selection, null);
  assert.deepEqual(parseGitView({ selection: { path: "a", staged: "yes" } }).selection, null);
  assert.deepEqual(parseGitView({ changesFolds: ["dir:src", 7, "", "dir:src", null, "dir:lib"] }).changesFolds, ["dir:src", "dir:lib"], "words only, each once");
  assert.deepEqual(parseGitView({ selection: { path: "a", staged: false, busy: "push" }, busy: "push" }), { selection: { path: "a", staged: false }, changesFolds: [], graphRefs: "all" }, "nothing beside the view comes back");
});

test("a session that only got busy has no new view to keep", () => {
  const stood = { ...EMPTY_SESSION, selection: { path: "a", staged: false } };
  assert.equal(sameGitView(stood, { ...stood, busy: "push", stale: 2 }), true);
  assert.equal(sameGitView(stood, { ...stood, selection: { path: "b", staged: false } }), false);
  assert.equal(sameGitView(stood, { ...stood, changesFolds: ["dir:src"] }), false);
  assert.equal(sameGitView(stood, { ...stood, graphRefs: "head" }), false);
});

const row = (path, extra = {}) => ({ path, index: ".", worktree: "M", staged: false, unstaged: true, untracked: false, conflicted: false, ...extra });

test("a write's answer lands the rows, keeps a selection that still exists, clears the failure and marks the reads stale", () => {
  const before = { ...EMPTY_SESSION, selection: { path: "a.txt", staged: false }, failure: failureOf("stage", "boom", ["a.txt"]) };
  const after = applied(before, [row("a.txt"), row("b.txt")]);
  assert.deepEqual(after.selection, { path: "a.txt", staged: false });
  assert.equal(after.failure, null);
  assert.equal(after.files.length, 2);
  assert.equal(after.stale, 1, "mounted readers reload once");
  const gone = applied(before, [row("b.txt")]);
  assert.equal(gone.selection, null, "a selection of a row that left the list is dropped");
  assert.equal(EMPTY_SESSION.busy, null, "a fresh session is idle");
  assert.equal(EMPTY_SESSION.amend, false, "the composer commits unless the switch says amend");
  assert.deepEqual([...EMPTY_SESSION.changesFolds], [], "a fresh session has every section and folder open");
  assert.equal(EMPTY_SESSION.operation, null, "no operation of this run is half-done");
});

test("a failure is a descriptor, never a closure, so the panel that meets it later can retry", () => {
  const f = failureOf("discard", "refused", ["x", "y"]);
  assert.deepEqual(f, { kind: "discard", error: "refused", paths: ["x", "y"] });
  assert.equal(typeof f.kind, "string");
  assert.deepEqual(failureOf("commit", "nothing to commit").paths, []);
});

test("sessions are remembered per scope up to a cap, the oldest touched dropped first", () => {
  let all = {};
  for (let i = 0; i < MAX_SESSIONS + 3; i++) all = remember(all, `workstream:${i}`, { n: i });
  assert.equal(Object.keys(all).length, MAX_SESSIONS);
  assert.equal(all["workstream:0"], undefined, "the first touched is gone");
  assert.equal(all[`workstream:${MAX_SESSIONS + 2}`].n, MAX_SESSIONS + 2);
  const touched = remember(all, "workstream:5", { n: "again" });
  assert.equal(Object.keys(touched).at(-1), "workstream:5", "touching moves a scope to the end");
});

test("the keys: a per-scope localStorage family, and a per-file draft that changes with the file's content", () => {
  assert.equal(draftStorageKey("workstream:abc"), "bisa:git:workstream:abc");
  assert.notEqual(fileDraftKey("s", "a.txt", "conflict:h1"), fileDraftKey("s", "a.txt", "conflict:h2"), "a file that moved on disk is a different draft");
  assert.notEqual(fileDraftKey("s", "a.txt", "hunks:true:h"), fileDraftKey("s", "a.txt", "hunks:false:h"), "the two sides of a file are two drafts");
  assert.equal(fingerprint("abc"), fingerprint("abc"));
  assert.notEqual(fingerprint("abc"), fingerprint("abd"), "a changed text is a changed facet");
  assert.match(fingerprint(""), /^0:/);
});

test("one component's own drafts are bounded: a key a file's hash leaves behind on every save is let go, least recently touched first", () => {
  assert.ok(Number.isInteger(MAX_SESSION_DRAFTS) && MAX_SESSION_DRAFTS >= 128, "room for every draft a long session has open at once");
  const store = readFileSync(new URL("./gitPanelStore.ts", import.meta.url), "utf8");
  assert.ok(store.includes("new Lru<unknown>(MAX_SESSION_DRAFTS)"), "the memory tier is an LRU under the model's cap, not a Map that grows for the life of the window");
  assert.ok(!/const memory = new Map/.test(store));
});
