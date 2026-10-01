/**
 * Git › Changes as a person works a checkout (ide/04): git's letters become
 * rows with one standing each, the tree nests them as on disk with a
 * folder's verb taking exactly the files it applies to, staging moves a file
 * between the sides, Commit is held until something is staged and a message
 * is typed — and never with an unmerged index — the publishing policy names
 * what happens after, and a merge ends in the after-merge plan whose steps
 * are the settings' and whose summary is the outcomes'. No DOM.
 *
 * Run with `node --test desktop/src/scenarios/gitPanel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { commitBlockedReason, groupGitFiles, isClean, rowActions, stageScopes, standingOf } from "../views/_work/gitFiles.mjs";
import { changesTreeRows, dirId, fileId, folderActions } from "../views/_work/gitTreeModel.mjs";
import { PUBLISH_LABEL, PUBLISH_POLICIES, publishOf } from "../views/_work/publishPolicy.mjs";
import { afterMergePlan, stepsToRun, summaryOf, toggleStep } from "../views/_work/afterMergeModel.mjs";
import { INIT_LABEL, PLAIN_FOLDER, initOffer } from "../views/_work/initRepositoryModel.mjs";
import { movesStatuses } from "../shell/workstreamFramesModel.mjs";
import { reloadsWorkspace } from "../shell/workspaceLoadModel.mjs";
import { afterPull, pullBannerWords } from "../views/_work/syncModel.mjs";
import { MAX_SESSION_DRAFTS } from "../views/_work/gitPanelModel.mjs";
import { misfits } from "./schemaFit.mjs";

const file = (path, over = {}) => ({ path, index: ".", worktree: "M", staged: false, unstaged: true, untracked: false, conflicted: false, ...over });
const untracked = (path) => file(path, { index: "?", worktree: "?", unstaged: true, untracked: true });
const staged = (path) => file(path, { index: "A", worktree: ".", staged: true, unstaged: false });
const conflicted = (path) => file(path, { index: "U", worktree: "U", staged: false, unstaged: false, conflicted: true });
const identity = { name: "Ada", email: "ada@example.test", source: "local" };
const NONE = new Set();

test("from a dirty tree to a commit: the rows, the tree, a stage, the held Commit, the message, the commit", () => {
  let files = [file("src/app.ts"), untracked("src/new.ts"), file("README.md")];
  let groups = groupGitFiles(files);
  assert.equal(isClean(groups), false);
  assert.deepEqual(groups.unstaged.map((f) => f.path), ["README.md", "src/app.ts"], "sorted by path within a side");
  assert.deepEqual(groups.untracked.map((f) => f.path), ["src/new.ts"]);
  assert.deepEqual(groups.staged, []);

  // The tree nests as on disk; the folder's Stage takes exactly its two stageable files.
  const rows = changesTreeRows(files, "tree", NONE);
  const src = rows.find((r) => r.id === dirId("src"));
  assert.deepEqual(src.stageable, ["src/app.ts", "src/new.ts"]);
  assert.ok(rows.some((r) => r.id === fileId("README.md")));
  const verbs = folderActions(src).map((a) => a.id);
  assert.ok(verbs.includes("stage"), `a folder offers Stage: ${verbs.join(", ")}`);

  // Commit is held: nothing staged.
  let scopes = stageScopes(groups);
  assert.equal(scopes.staged.count, 0);
  assert.equal(scopes.all.count, 3);
  assert.equal(commitBlockedReason({ message: "Fix", groups, exists: true, git: true, identity }), "Nothing is staged. Stage a file, and Commit records exactly that.");

  // Stage one file: it moves sides; the row's actions turn round.
  const row = files[0];
  assert.deepEqual(rowActions(standingOf(row)).map((a) => a.id), ["stage", "discard"]);
  files = [file("src/app.ts", { index: "M", worktree: ".", staged: true, unstaged: false }), untracked("src/new.ts"), file("README.md")];
  groups = groupGitFiles(files);
  assert.deepEqual(groups.staged.map((f) => f.path), ["src/app.ts"]);
  assert.deepEqual(rowActions(standingOf(files[0])).map((a) => a.id), ["unstage"]);
  scopes = stageScopes(groups);
  assert.equal(scopes.staged.count, 1);

  // Held for a message, then open.
  assert.equal(commitBlockedReason({ message: "  ", groups, exists: true, git: true, identity }), "A commit needs a message.");
  assert.equal(commitBlockedReason({ message: "Fix the total", groups, exists: true, git: true, identity }), null);
  // A conflict anywhere holds it whatever is staged; a missing identity holds it before anything.
  assert.match(commitBlockedReason({ message: "Fix", groups: groupGitFiles([...files, conflicted("lib/x.rs")]), exists: true, git: true, identity }), /Resolve the conflicted files first/);
  assert.match(commitBlockedReason({ message: "Fix", groups, exists: true, git: true, identity: { name: null, email: null, source: "missing" } }), /Nobody is set to commit/);
  // A plain folder and a folder gone from disk have their own words.
  assert.match(commitBlockedReason({ message: "Fix", groups, exists: true, git: false, identity }), /plain folder/);
  assert.match(commitBlockedReason({ message: "Fix", groups, exists: false, git: true, identity }), /not on disk/);
  // Amend: no commit yet is the one extra hold; nothing staged is fine — a reword.
  assert.equal(commitBlockedReason({ message: "Reword", groups: groupGitFiles([]), exists: true, git: true, identity, head: null, amend: true }), "No commit to amend yet.");
  assert.equal(commitBlockedReason({ message: "Reword", groups: groupGitFiles([]), exists: true, git: true, identity, head: "abc", amend: true }), null);
});

test("what happens after: the project's publishing policy is named, and a merge ends in the after-merge plan — every step the settings ask for, toggled by hand, summed up from what ran", () => {
  assert.deepEqual([...PUBLISH_POLICIES], ["manual", "gated", "auto"]);
  assert.equal(publishOf({ publish: "gated" }), "gated");
  assert.equal(publishOf({}), "auto", "the core's default when the record carries none");
  for (const p of PUBLISH_POLICIES) assert.ok(PUBLISH_LABEL[p], `${p} has a label`);

  const base = { cleanup: "ask", afterMerge: "ask", pullMode: "ff_only", primaryBusy: 0, defaultBranch: "main", branch: "work/dark-mode" };
  const plan = afterMergePlan(base);
  assert.equal(plan.mode, "dialog", "ask + ask is a dialog");
  const ids = plan.steps.map((s) => s.id);
  assert.ok(ids.includes("pull") && ids.length >= 2, `the steps: ${ids.join(", ")}`);
  assert.ok(plan.steps.every((s) => s.checked && s.available), "every step checked and open when the primary is idle");
  // Uncheck one, and only that one leaves the run.
  const toggled = toggleStep(plan.steps, "pull");
  assert.deepEqual(stepsToRun(toggled), stepsToRun(plan.steps).filter((id) => id !== "pull"), "the ids to run, in running order");
  assert.deepEqual(stepsToRun(toggleStep(toggled, "pull")), stepsToRun(plan.steps), "toggled back");
  assert.ok(stepsToRun(plan.steps).includes("pull"));
  // A busy primary holds the pull with its reason; the rest still runs.
  const busy = afterMergePlan({ ...base, primaryBusy: 2 });
  const pull = busy.steps.find((s) => s.id === "pull");
  assert.equal(pull.available, false);
  assert.match(pull.reason, /2 sessions run on main/);
  // Silent settings: no dialog, the steps run as set.
  assert.equal(afterMergePlan({ ...base, cleanup: "delete", afterMerge: "return" }).mode, "silent");
  assert.equal(afterMergePlan({ ...base, cleanup: "keep", afterMerge: "stay" }).mode, "none");
  // The summary is the outcomes', in the person's words.
  const summary = summaryOf({ pull: "done", delete: "done", return: "done" }, "main");
  assert.equal(typeof summary, "string");
  assert.ok(summary.length > 0);
  assert.notEqual(summaryOf({ pull: "conflict" }, "main"), summary, "a conflict reads differently from a clean run");
});

// A plain folder's one offer (ide/04, ide/07, ide/13): the Git panel, the
// Workstreams panel and About › Checkout each draw `InitRepositoryCard`, the
// sentence lives once — in the model — and `GitFacts` stays a reading with
// no button, since the Projects list reuses it.
test("a plain folder gets one card on three surfaces, and its sentence has one home", () => {
  const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const surfaces = {
    "the Git panel": read("../views/_workbench/RightPanel.tsx"),
    "the Workstreams panel": read("../views/_work/WorkstreamPanel.tsx"),
    "About › Checkout": read("../views/_work/CheckoutView.tsx"),
  };
  for (const [name, src] of Object.entries(surfaces)) {
    assert.ok(src.includes("<InitRepositoryCard"), `${name} draws the card`);
  }
  const card = read("../views/_work/InitRepositoryCard.tsx");
  assert.ok(card.includes("{PLAIN_FOLDER}") && card.includes("INIT_LABEL"), "the card reads its words from the model");
  assert.ok(card.includes("api.initRepository(") && card.includes("refreshWorkstreamStatuses()"), "the card owns the action and tells the rail");
  assert.ok(card.includes("<ConfirmDialog"), "an adopted folder is asked about first");
  const literal = PLAIN_FOLDER.slice(0, 32);
  for (const [name, src] of Object.entries({ ...surfaces, GitFacts: read("../views/_work/ProjectDetail.tsx"), card })) {
    assert.ok(!src.includes(literal), `${name} does not spell the sentence itself`);
  }
  const facts = read("../views/_work/ProjectDetail.tsx");
  assert.ok(facts.includes("{PLAIN_FOLDER}"), "GitFacts reads the sentence from the model");
  assert.ok(!facts.includes("<InitRepositoryCard"), "GitFacts stays a reading — the Projects list reuses it");
  assert.equal(initOffer({ kind: "primary", exists: true, git: false }).shown, true);
  assert.equal(initOffer({ kind: "copy", exists: true, git: false }).shown, false, "a copy never carries the button");
  assert.equal(INIT_LABEL, "Initialise a repository");
});

test("a commit from Git › Changes is a commit: the body names what was chosen, and the record the node moves reaches the rail, the Board and the panel by the frames the workstream's own commit raises", () => {
  const schema = JSON.parse(readFileSync(new URL("../../api-schema.json", import.meta.url), "utf8"));
  // What `api.gitCommit` sends: the message, and the paths only when some were chosen — an empty list would commit what is staged, never everything.
  assert.deepEqual(misfits(schema, "WorkstreamCommitBody", { message: "Add the parser" }), []);
  assert.deepEqual(misfits(schema, "WorkstreamCommitBody", { message: "Add the parser", paths: ["src/parser.rs"] }), []);
  assert.notDeepEqual(misfits(schema, "WorkstreamCommitBody", { message: "x", staged: true }), [], "a key the node does not declare is refused");
  const client = readFileSync(new URL("../api.ts", import.meta.url), "utf8");
  assert.ok(client.includes("paths && paths.length > 0 ? { message, paths } : { message }"), "no `paths` on an empty choice");
  // The node moves the record and says so as for `POST /workstreams/{wid}/commit`: nothing on the desktop decides the state itself.
  for (const type of ["workstream_changed", "workstream_committed"]) {
    assert.equal(movesStatuses(type), true, `${type} re-reads every status — the rail's mark, the Board's chip`);
  }
  assert.equal(reloadsWorkspace("workstream_changed"), true, "the Board's column follows the record the index reads again");
  const ops = readFileSync(new URL("../views/_work/gitOps.ts", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
  assert.ok(!/state:\s*"committed"/.test(ops), "gitOps writes no workstream state of its own");
});

test("a pull's outcome is one banner in the catalog's words; the session's own drafts are bounded", () => {
  assert.equal(pullBannerWords(afterPull({ err: { status: 409, code: "not_fast_forward", message: "x", detail: { ahead: 2, behind: 1 } } })).body, "This branch has 2 commits of its own and the upstream has 1. Nothing moved. Choose how to bring them together:");
  assert.equal(pullBannerWords(afterPull({ err: { status: 409, code: "conflict", message: "x", detail: { paths: ["a.rs"], in_progress: "merge" } } })).title, "The merge stopped on 1 file.");
  assert.ok(MAX_SESSION_DRAFTS >= 128);
  // The changed-files store keeps the rows of the few roots left last, reads every shown root again when the node comes back, and lands rows only for a root it knows.
  const store = readFileSync(new URL("../views/_work/gitFilesStore.ts", import.meta.url), "utf8");
  assert.match(store, /const MAX_IDLE_ROOTS = 8;/);
  assert.ok(store.includes("if (e.subscribers === 0) prune();"), "a root nobody shows any more is a candidate to let go");
  assert.ok(store.includes("reloadOnReconnect(watchConnection, readShown)"), "what changed while the node was away was said by no frame");
  assert.ok(store.includes("const e = entries.get(scope);\n  if (!e) return;"), "rows landing for a root nobody showed make no entry");
});
