/**
 * What the project git panel decides before it draws anything.
 *
 * Nothing here renders — there is no jsdom in this repo, which is why these
 * decisions are a module rather than a pile of `useState` reads. The ones
 * that can actually be wrong in a way a person notices: which list a file
 * lands in when git has two different things to say about it, whether the
 * patch on screen follows the file it belongs to, whether Commit is offered
 * when there is nothing staged, and what a failed suggestion leaves in the
 * box.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  commitBlockedReason,
  diffStat,
  gitFileMenu,
  groupGitFiles,
  isClean,
  isSelected,
  keepSelection,
  KIND_MARK,
  kindForLetter,
  letterPair,
  noCommitsYet,
  rowActions,
  selectionOf,
  stageScopes,
  standingOf,
  suggestionOutcome,
  treeStandings,
} from "./gitFiles.mjs";

/** A row as `GET /projects/{pid}/git/files` reports one. */
const row = (path, index, worktree, extra = {}) => ({
  path,
  old_path: null,
  index,
  worktree,
  staged: !extra.untracked && !extra.conflicted && index !== ".",
  unstaged: !!extra.untracked || (!extra.conflicted && worktree !== "."),
  untracked: !!extra.untracked,
  conflicted: !!extra.conflicted,
  ...extra,
});

test("a file that is staged and edited since is in both lists, not one", () => {
  // The whole reason `index` and `worktree` are separate on the wire. A panel
  // that picked a winner would show somebody a staged file and hide the fact
  // that what they are about to commit is not what is on disk.
  const groups = groupGitFiles([row("src/a.rs", "M", "M")]);
  assert.deepEqual(
    groups.staged.map((f) => f.path),
    ["src/a.rs"],
  );
  assert.deepEqual(
    groups.unstaged.map((f) => f.path),
    ["src/a.rs"],
  );
});

test("an untracked file is untracked and nothing else", () => {
  // `is_unstaged()` is true for an untracked path — it is unstaged by
  // definition — so a naive read puts a brand-new file in "Unstaged" as
  // well, where staging it means something different from what the label says.
  const groups = groupGitFiles([row("notes.md", "?", "?", { untracked: true })]);
  assert.equal(groups.untracked.length, 1);
  assert.equal(groups.unstaged.length, 0);
  assert.equal(groups.staged.length, 0);
});

test("a conflicted file has a list of its own rather than none", () => {
  // `is_staged()` and `is_unstaged()` are both false for an unmerged path, so
  // a three-way split drops it off the screen entirely — the worst outcome
  // for the one state that most needs somebody to look at it.
  const groups = groupGitFiles([row("src/merge.rs", "U", "U", { conflicted: true })]);
  assert.equal(groups.conflicted.length, 1);
  assert.equal(groups.staged.length + groups.unstaged.length + groups.untracked.length, 0);
});

test("rows are ordered by path so nothing moves under the pointer", () => {
  const groups = groupGitFiles([row("z.rs", ".", "M"), row("a.rs", ".", "M")]);
  assert.deepEqual(
    groups.unstaged.map((f) => f.path),
    ["a.rs", "z.rs"],
  );
});

test("junk in the list is skipped rather than drawn as a blank row", () => {
  const groups = groupGitFiles([null, { path: "" }, row("ok.rs", ".", "M")]);
  assert.equal(groups.unstaged.length, 1);
  assert.equal(isClean(groups), false);
  assert.equal(isClean(groupGitFiles([])), true);
  assert.equal(isClean(groupGitFiles(null)), true);
});

test("a file's standing says which sides it has, and the side a click opens — the working tree when there is one", () => {
  assert.deepEqual(standingOf(row("src/a.rs", "A", "M")), { staged: true, unstaged: true, untracked: false, conflicted: false, primary: "unstaged" });
  assert.deepEqual(standingOf(row("src/a.rs", "A", ".")), { staged: true, unstaged: false, untracked: false, conflicted: false, primary: "staged" });
  assert.deepEqual(standingOf(row("src/a.rs", ".", "M")), { staged: false, unstaged: true, untracked: false, conflicted: false, primary: "unstaged" });
  assert.deepEqual(standingOf(row("n.md", "?", "?", { untracked: true })), { staged: false, unstaged: false, untracked: true, conflicted: false, primary: "unstaged" });
  assert.deepEqual(standingOf(row("c.rs", "U", "U", { conflicted: true, staged: true })), { staged: false, unstaged: false, untracked: false, conflicted: true, primary: "unstaged" }, "an unmerged path is neither side, whatever the flags say");
  assert.deepEqual(standingOf(null), { staged: false, unstaged: false, untracked: false, conflicted: false, primary: "unstaged" });
});

test("git's letters read as the same vocabulary the workstream panel uses", () => {
  assert.equal(kindForLetter("R"), "renamed");
  assert.equal(kindForLetter("."), null, "an unmodified side has no kind");
  assert.equal(kindForLetter(""), null);
  assert.equal(kindForLetter("Z"), null, "a letter git has not defined is not guessed at");
  assert.equal(letterPair(row("a.rs", "M", ".")), "M.");
  assert.equal(letterPair(null), "..");
});

test("a selection names the path and which of its two patches is open", () => {
  const f = row("src/a.rs", "M", "M");
  assert.deepEqual(selectionOf(f, "staged"), { path: "src/a.rs", staged: true });
  assert.deepEqual(selectionOf(f, "unstaged"), { path: "src/a.rs", staged: false });
  assert.deepEqual(selectionOf(row("n.md", "?", "?", { untracked: true }), "unstaged"), { path: "n.md", staged: false });
  assert.equal(selectionOf(null, "staged"), null);

  const sel = selectionOf(f, "staged");
  assert.equal(isSelected(sel, f, "staged"), true);
  assert.equal(isSelected(sel, f, "unstaged"), false, "the same file on the other side is not it");
  assert.equal(isSelected(null, f, "staged"), false);
});

test("staging the file you are reading keeps its patch on screen", () => {
  // Before: the worktree diff. After: the working tree matches the index and
  // that patch is empty, so the selection follows the file to the side that
  // still has something to show rather than going blank.
  const before = { path: "src/a.rs", staged: false };
  const after = [row("src/a.rs", "M", ".")];
  assert.deepEqual(keepSelection(before, after), { path: "src/a.rs", staged: true });
});

test("unstaging the file you are reading follows it back the other way", () => {
  const before = { path: "src/a.rs", staged: true };
  const after = [row("src/a.rs", ".", "M")];
  assert.deepEqual(keepSelection(before, after), { path: "src/a.rs", staged: false });
});

test("a file that left the list drops the selection rather than pointing at nothing", () => {
  assert.equal(keepSelection({ path: "src/a.rs", staged: true }, []), null);
  assert.equal(keepSelection(null, [row("src/a.rs", "M", ".")]), null);
});

test("a selection that is still valid is left exactly as it was", () => {
  const sel = { path: "src/a.rs", staged: true };
  const files = [row("src/a.rs", "M", "M")];
  assert.deepEqual(keepSelection(sel, files), sel);
});

test("commit is offered only once something is staged and a message is written", () => {
  const groups = groupGitFiles([row("src/a.rs", "M", ".")]);
  assert.equal(commitBlockedReason({ message: "fix it", groups, exists: true, git: true }), null);
});

test("nothing staged blocks commit even with a whole tree of changes", () => {
  // The route treats an empty `paths` as "commit what is already staged",
  // never as "commit everything" — so a Commit offered here would be a 409
  // dressed up as a button.
  const groups = groupGitFiles([row("src/a.rs", ".", "M"), row("n.md", "?", "?", { untracked: true })]);
  assert.match(
    commitBlockedReason({ message: "fix it", groups, exists: true, git: true }),
    /Nothing is staged/,
  );
});

test("amend needs a commit and a message, and nothing staged is a reword rather than a block", () => {
  const nothing = groupGitFiles([row("src/a.rs", ".", "M")]);
  assert.equal(commitBlockedReason({ message: "reworded", groups: nothing, exists: true, git: true, head: "abc123", amend: true }), null, "a reword folds nothing in");
  assert.match(commitBlockedReason({ message: "reworded", groups: nothing, exists: true, git: true, head: null, amend: true }), /No commit to amend/);
  assert.match(commitBlockedReason({ message: "  ", groups: nothing, exists: true, git: true, head: "abc123", amend: true }), /needs a message/);
  const conflicted = groupGitFiles([row("src/a.rs", "U", "U", { conflicted: true })]);
  assert.match(commitBlockedReason({ message: "x", groups: conflicted, exists: true, git: true, head: "abc123", amend: true }), /conflicted/);
  assert.match(commitBlockedReason({ message: "x", groups: nothing, exists: true, git: true, head: null }), /Nothing is staged/, "without the switch, no head is not the reason");
});

test("an all-whitespace message is no message", () => {
  const groups = groupGitFiles([row("src/a.rs", "M", ".")]);
  assert.match(
    commitBlockedReason({ message: "   \n ", groups, exists: true, git: true }),
    /needs a message/,
  );
});

test("a plain folder and a missing folder each say which they are", () => {
  assert.match(commitBlockedReason({ message: "x", groups: null, exists: false }), /not on disk/);
  assert.match(
    commitBlockedReason({ message: "x", groups: null, exists: true, git: false }),
    /plain folder/,
  );
});

test("a conflict blocks the commit before the message does", () => {
  const groups = groupGitFiles([
    row("src/a.rs", "M", "."),
    row("src/m.rs", "U", "U", { conflicted: true }),
  ]);
  assert.match(
    commitBlockedReason({ message: "fix it", groups, exists: true, git: true }),
    /Resolve the conflicted files first/,
  );
});

test("a suggested message arrives trimmed and ready to edit", () => {
  const outcome = suggestionOutcome({
    project: "p",
    suggested: true,
    message: "  Fix the cart total rounding\n",
    agent: "general-agent",
    error: null,
  });
  assert.equal(outcome.message, "Fix the cart total rounding");
  assert.equal(outcome.note, null);
});

test("a failed suggestion leaves the box alone and says why", () => {
  // The route is always 200, so `suggested: false` is the no-harness or
  // timed-out case. `message: null` means "do not touch what is in the box":
  // returning "" would clear whatever the person had already typed, and
  // returning anything else would put words in a commit nobody wrote.
  const outcome = suggestionOutcome({
    project: "p",
    suggested: false,
    message: "",
    agent: "general-agent",
    error: "no harness is configured",
  });
  assert.equal(outcome.message, null);
  assert.match(outcome.note, /no harness is configured/);
});

test("a suggestion with no error still refuses to invent one", () => {
  assert.equal(suggestionOutcome(null).message, null);
  assert.match(suggestionOutcome(null).note, /did not say why/);
  const empty = suggestionOutcome({ suggested: true, message: "   " });
  assert.equal(empty.message, null);
  assert.match(empty.note, /empty message/);
});

test("line counts come from the patch, and file headers are not content", () => {
  const patch = [
    "--- a/src/a.rs",
    "+++ b/src/a.rs",
    "@@ -1,3 +1,3 @@",
    " context",
    "-gone",
    "+added",
    "+added again",
  ].join("\n");
  assert.deepEqual(diffStat(patch), { insertions: 2, deletions: 1 });
  assert.deepEqual(diffStat(""), { insertions: 0, deletions: 0 });
  assert.deepEqual(diffStat(null), { insertions: 0, deletions: 0 });
});

test("a missing commit identity blocks the commit ahead of the message, and an unloaded one does not", () => {
  const groups = { staged: [{ path: "a.txt" }], unstaged: [], untracked: [], conflicted: [] };
  assert.match(
    commitBlockedReason({ message: "fix it", groups, exists: true, git: true, identity: { source: "none" } }),
    /Nobody is set to commit/,
  );
  assert.equal(
    commitBlockedReason({ message: "fix it", groups, exists: true, git: true, identity: { source: "global" } }),
    null,
  );
  assert.equal(commitBlockedReason({ message: "fix it", groups, exists: true, git: true, identity: null }), null);
});

test("a repository with no commits is told apart by its absent head, not its branch", () => {
  // `git status --porcelain=v2` names the branch on an unborn HEAD — it
  // reports `# branch.oid (initial)` beside a real `# branch.head main` — so
  // reading the branch name would say this repository has a history.
  assert.equal(noCommitsYet({ git: true, detached: false, branch: "main", head: null }), true);
  assert.equal(noCommitsYet({ git: true, detached: false, branch: "main", head: "abc1234" }), false);
  assert.equal(noCommitsYet({ git: true, detached: true, branch: null, head: "abc1234" }), false);
  assert.equal(noCommitsYet({ git: false, detached: false, head: null }), false);
  assert.equal(noCommitsYet(null), false);
});

test("a changed file's menu opens it, stages and/or unstages it by its standing, discards or deletes it, copies its paths and reveals it — the absolute path only under the shell", () => {
  const unstaged = standingOf(row("a", ".", "M"));
  const staged = standingOf(row("a", "A", "."));
  const both = standingOf(row("a", "M", "M"));
  const untracked = standingOf(row("a", "?", "?", { untracked: true }));
  const conflicted = standingOf(row("a", "U", "U", { conflicted: true }));
  const ids = (ctx) => gitFileMenu({ standing: unstaged, desktop: true, canOpen: true, ...ctx }).map((i) => i.id);
  assert.deepEqual(ids({}), ["open", "attach-agent", "stage", "discard", "copy-path", "copy-absolute", "reveal-files"], "a working-tree change can be discarded");
  assert.deepEqual(ids({ standing: staged }), ["open", "attach-agent", "unstage", "copy-path", "copy-absolute", "reveal-files"], "the index is not discarded from — unstage first");
  assert.deepEqual(ids({ standing: both }), ["open", "attach-agent", "stage", "unstage", "discard", "copy-path", "copy-absolute", "reveal-files"], "a file staged and edited since offers both toggles");
  assert.deepEqual(ids({ standing: untracked }), ["open", "attach-agent", "stage", "delete", "copy-path", "copy-absolute", "reveal-files"], "an untracked file is deleted, not discarded");
  assert.deepEqual(ids({ standing: conflicted }), ["open", "attach-agent", "stage", "discard", "copy-path", "copy-absolute", "reveal-files"], "staging an unmerged path marks it resolved; discarding puts it back to HEAD");
  assert.deepEqual(ids({ desktop: false }), ["open", "attach-agent", "stage", "discard", "copy-path", "reveal-files"]);
  assert.ok([unstaged, staged, both, untracked, conflicted].every((st) => !gitFileMenu({ standing: st, desktop: true, canOpen: true }).find((i) => i.id === "attach-agent").disabled), "every standing can be attached to the agent, an untracked file as a file chip");
  assert.ok(!ids({}).includes("stash") && !ids({ standing: staged }).includes("stash"), "stashing is the Stashes view's, never a row's");
  const items = gitFileMenu({ standing: unstaged, desktop: true, canOpen: false });
  assert.ok(items.find((i) => i.id === "open").disabled && items.find((i) => i.id === "reveal-files").disabled, "a deleted file has no document to open");
  assert.equal(items.find((i) => i.id === "reveal-files").command, "reveal_in_files");
  assert.ok(items.find((i) => i.id === "stage").separatorBefore && items.find((i) => i.id === "copy-path").separatorBefore);
  assert.ok(gitFileMenu({ standing: staged, desktop: true, canOpen: true }).find((i) => i.id === "unstage").separatorBefore, "the first toggle opens its group");
  assert.ok(!gitFileMenu({ standing: both, desktop: true, canOpen: true }).find((i) => i.id === "unstage").separatorBefore, "the second toggle sits with the first");
  assert.ok(items.find((i) => i.id === "discard").danger && items.find((i) => i.id === "discard").separatorBefore, "discard opens the destructive group, red");
  assert.equal(items.find((i) => i.id === "discard").label, "Discard changes…", "an ellipsis: a dialog asks first");
  assert.equal(gitFileMenu({ standing: untracked, desktop: false, canOpen: true }).find((i) => i.id === "delete").label, "Delete file…");
});

test("a row reveals its actions on hover from its standing: the toggles first — both for a file staged and edited since — then discard for a working-tree or unmerged change or delete for an untracked file, and only Unstage on a staged-only one", () => {
  const of = (r) => rowActions(standingOf(r)).map((a) => a.id);
  assert.deepEqual(of(row("a", ".", "M")), ["stage", "discard"]);
  assert.deepEqual(of(row("a", "U", "U", { conflicted: true })), ["stage", "discard"], "a discard puts an unmerged path back to HEAD");
  assert.deepEqual(of(row("a", "?", "?", { untracked: true })), ["stage", "delete"], "git has nothing to restore an untracked file to");
  assert.deepEqual(of(row("a", "A", ".")), ["unstage"], "the index is not discarded from — unstage first");
  assert.deepEqual(of(row("a", "M", "M")), ["stage", "unstage", "discard"], "both sides, both toggles, and the working tree's throw-away");
  for (const r of [row("a", ".", "M"), row("a", "A", "."), row("a", "M", "M"), row("a", "?", "?", { untracked: true }), row("a", "U", "U", { conflicted: true })]) {
    for (const a of rowActions(standingOf(r))) {
      assert.equal(a.icon, a.id, `${a.id} draws its own glyph`);
      assert.ok(a.label("src/x.rs").includes("src/x.rs"), `${a.id}'s label names the path`);
      assert.ok(a.hint.length > 20, `${a.id} has a sentence for its tooltip`);
      assert.equal(a.tone, a.id === "discard" || a.id === "delete" ? "danger" : "quiet", "only a throw-away is red");
    }
  }
  assert.match(rowActions(standingOf(row("a", "U", "U", { conflicted: true })))[1].hint, /HEAD/, "an unmerged path goes back to HEAD, and the hint says so");
  assert.match(rowActions(standingOf(row("a", "U", "U", { conflicted: true })))[0].hint, /resolved/, "staging it marks it resolved, and the hint says so");
  assert.match(rowActions(standingOf(row("a", ".", "M")))[1].hint, /index/, "a working-tree change goes back to the index");
  assert.match(rowActions(standingOf(row("a", "?", "?", { untracked: true })))[1].hint, /disposal/);
});

test("the stage scopes: all is the modified and the untracked together, tracked the modified alone, untracked the new files, staged what Unstage all takes back — a conflicted path in none; discardable the working-tree changes and the unmerged paths", () => {
  const groups = groupGitFiles([
    row("a.rs", "M", "M"),
    row("b.rs", ".", "M"),
    row("c.rs", "A", "."),
    row("new.txt", "?", "?", { untracked: true, staged: false, unstaged: true }),
    row("x.rs", "U", "U", { conflicted: true, staged: false, unstaged: false }),
  ]);
  const scopes = stageScopes(groups);
  assert.deepEqual(scopes.all, { paths: ["a.rs", "b.rs", "new.txt"], count: 3 });
  assert.deepEqual(scopes.tracked, { paths: ["a.rs", "b.rs"], count: 2 }, "git add -u's meaning");
  assert.deepEqual(scopes.untracked, { paths: ["new.txt"], count: 1 });
  assert.deepEqual(scopes.staged, { paths: ["a.rs", "c.rs"], count: 2 }, "a file in both lists counts once on each side");
  assert.ok(![scopes.all, scopes.tracked, scopes.untracked, scopes.staged].some((s) => s.paths.includes("x.rs")), "staging an unmerged path would mark it resolved");
  assert.deepEqual(scopes.discardable, { paths: ["a.rs", "b.rs", "x.rs"], count: 3 }, "Discard all changes… puts the working tree back, an unmerged path to HEAD; an untracked file is not git's to discard");
  assert.deepEqual(stageScopes(null).all, { paths: [], count: 0 });
  assert.deepEqual(stageScopes(null).discardable, { paths: [], count: 0 });
});

test("the explorer's standings read the working tree's letter first, the index's when the tree is quiet, and a conflict or an untracked file as itself", () => {
  const rows = [
    { path: "src/a.rs", index: ".", worktree: "M" },
    { path: "src/b.rs", index: "A", worktree: "." },
    { path: "src/c.rs", index: "A", worktree: "M" },
    { path: "src/d.rs", index: "R", worktree: ".", old_path: "src/old.rs" },
    { path: "src/e.rs", index: "D", worktree: "." },
    { path: "src/f.rs", index: "C", worktree: "." },
    { path: "src/g.rs", index: "T", worktree: "." },
    { path: "src/h.rs", index: "?", worktree: "?", untracked: true },
    { path: "src/i.rs", index: "U", worktree: "U", conflicted: true },
    { path: "src/quiet.rs", index: ".", worktree: "." },
    { path: "", index: "M", worktree: "M" },
  ];
  assert.deepEqual(treeStandings(rows), [
    { path: "src/a.rs", kind: "modified", staged: false },
    { path: "src/b.rs", kind: "added", staged: true },
    { path: "src/c.rs", kind: "modified", staged: false },
    { path: "src/d.rs", kind: "renamed", staged: true },
    { path: "src/e.rs", kind: "deleted", staged: true },
    { path: "src/f.rs", kind: "added", staged: true },
    { path: "src/g.rs", kind: "modified", staged: true },
    { path: "src/h.rs", kind: "untracked", staged: false },
    { path: "src/i.rs", kind: "conflict", staged: false },
  ]);
  assert.deepEqual(treeStandings(null), []);
});
