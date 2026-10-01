/**
 * Opening a workstream: the guard every entry point shares, the sources and
 * the wire body each makes, the branch name preview (checked against the Rust
 * tests' own vectors — `crates/bisa-engine/tests/projects.rs`,
 * `branch_names_are_readable_and_always_legal`, and `typed_branch_name`),
 * the branches other checkouts hold, and the words once it is open.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { BRANCH_SLUG_CHARS, SOURCES, TAIL_PLACEHOLDER, branchNameFor, canOpenWorkstream, fieldsOf, labelShown, openBody, openWords, previewBranch, sanitizeRefComponent, sourceBody, takenBranches, typedBranchName } from "./workstreamCreation.mjs";

test("a project takes a workstream once it has a commit; a plain folder takes a copy", () => {
  assert.deepEqual(canOpenWorkstream({ exists: true }, { git: true, exists: true, head: "abc" }), { ok: true, mode: "branch" });
  assert.deepEqual(canOpenWorkstream({ exists: true }, { git: false, exists: true }), { ok: true, mode: "copy" });
  const unborn = canOpenWorkstream({ exists: true }, { git: true, exists: true, head: null });
  assert.equal(unborn.ok, false);
  assert.equal(unborn.reason, "unborn");
  assert.match(unborn.remedy, /first commit/);
  assert.equal(canOpenWorkstream({ exists: true }, null).reason, "loading");
  assert.equal(canOpenWorkstream({ exists: false }, null).reason, "missing");
  assert.equal(canOpenWorkstream({ exists: true }, { git: true, exists: false }).reason, "missing");
});

test("a ref component is made safe, never refused — the engine's rule; a typed name keeps its kind before the slash", () => {
  assert.equal(sanitizeRefComponent("Implement the parser", "item"), "implement-the-parser");
  assert.equal(sanitizeRefComponent("cart total (v2)", "item"), "cart-total-v2");
  assert.equal(sanitizeRefComponent("a..b", "item"), "a.b");
  assert.equal(sanitizeRefComponent("x.lock", "item"), "x");
  assert.equal(sanitizeRefComponent("-.-", "item"), "item", "nothing left: the fallback");
  assert.equal(sanitizeRefComponent("", "work"), "work");
  assert.equal(sanitizeRefComponent("Ünïcode ✓", "item"), "n-code", "non-ascii folds to dashes");
  assert.equal(branchNameFor("work", "Add analytics"), "work/add-analytics");
  assert.equal(branchNameFor("", "***"), "work/item");
  // `typed_branch_name`: kind/slug when a slash was typed, work/<slug> otherwise.
  assert.equal(typedBranchName("Feature/Dark Mode"), "feature/dark-mode");
  assert.equal(typedBranchName("feature/from tag"), "feature/from-tag");
  assert.equal(typedBranchName("hotfix"), "work/hotfix");
  assert.equal(typedBranchName("/lead/ing/"), "lead/ing");
  assert.equal(typedBranchName("trailing/"), "work/trailing");
});

test("the sources are five, in the dialog's order, each with a sentence", () => {
  assert.deepEqual(
    SOURCES.map((s) => s.id),
    ["new", "branch", "remote", "tag", "pr"],
  );
  for (const s of SOURCES) assert.ok(s.label && s.hint.endsWith("."), s.id);
});

test("the preview is what the engine settles on: forty characters of the label then the id tail for a derived name, the branch itself for the rest", () => {
  assert.equal(BRANCH_SLUG_CHARS, 40);
  assert.deepEqual(previewBranch({ label: "Implement the parser" }), { branch: `work/implement-the-parser-${TAIL_PLACEHOLDER}`, derived: true });
  assert.deepEqual(previewBranch({ label: "Rewrite the entire billing subsystem from scratch and also the invoices" }), {
    branch: `work/rewrite-the-entire-billing-subsystem-fro-${TAIL_PLACEHOLDER}`,
    derived: true,
  });
  assert.deepEqual(previewBranch({ label: "", project: "web-app" }), { branch: `work/web-app-${TAIL_PLACEHOLDER}`, derived: true }, "no label: the project's slug");
  assert.deepEqual(previewBranch({ label: "x", source: { kind: "new", name: "Feature/Dark Mode" } }), { branch: "feature/dark-mode", derived: false }, "a name of your own is made safe the engine's way");
  assert.deepEqual(previewBranch({ source: { kind: "branch", branch: "topic" } }), { branch: "topic", derived: false });
  assert.deepEqual(previewBranch({ source: { kind: "remote", remote: "origin", branch: "feature/x" } }), { branch: "feature/x", derived: false }, "the remote branch's local twin");
  assert.deepEqual(previewBranch({ source: { kind: "tag", tag: "v1.2.0" } }), { branch: "from/v1.2.0", derived: false }, "a tag's branch is from/<tag>");
  assert.deepEqual(previewBranch({ source: { kind: "tag", newTag: true, tagName: "v2", tagAt: "main" } }), { branch: "from/v2", derived: false });
  assert.deepEqual(previewBranch({ source: { kind: "tag", tag: "v1", branch: "Release/1" } }), { branch: "release/1", derived: false }, "unless a person named the branch");
  assert.deepEqual(previewBranch({ source: { kind: "pr", pr: 12, head: "feature/pr" } }), { branch: "feature/pr", derived: false }, "a pull request's head");
  assert.deepEqual(previewBranch({ source: { kind: "tag" } }), { branch: "", derived: false }, "nothing picked yet: nothing to preview");
});

test("each source makes its wire body, and an empty pick or a tag name git refuses is the one problem", () => {
  assert.deepEqual(sourceBody({ kind: "new" }), { source: { source: "new_branch", name: null, start: null }, problem: null });
  assert.deepEqual(sourceBody({ kind: "new", name: " feature/x ", start: "v1" }), { source: { source: "new_branch", name: "feature/x", start: "v1" }, problem: null });
  assert.deepEqual(sourceBody({ kind: "branch", branch: "topic" }), { source: { source: "local_branch", name: "topic" }, problem: null });
  assert.match(sourceBody({ kind: "branch" }).problem, /Pick a branch/);
  assert.deepEqual(sourceBody({ kind: "remote", remote: "origin", branch: "feature/x" }), { source: { source: "remote_branch", remote: "origin", name: "feature/x" }, problem: null });
  assert.match(sourceBody({ kind: "remote", remote: "origin" }).problem, /Pick a remote branch/);
  assert.deepEqual(sourceBody({ kind: "tag", tag: "v1" }), { source: { source: "tag", name: "v1", branch: null, create_at: null }, problem: null });
  assert.deepEqual(sourceBody({ kind: "tag", tag: "v1", branch: "release/1" }), { source: { source: "tag", name: "v1", branch: "release/1", create_at: null }, problem: null });
  assert.match(sourceBody({ kind: "tag" }).problem, /Pick a tag/);
  assert.deepEqual(sourceBody({ kind: "tag", newTag: true, tagName: "v2", tagAt: "main" }), { source: { source: "tag", name: "v2", branch: null, create_at: "main" }, problem: null });
  assert.match(sourceBody({ kind: "tag", newTag: true, tagAt: "main" }).problem, /Name the tag/);
  assert.match(sourceBody({ kind: "tag", newTag: true, tagName: "v 2", tagAt: "main" }).problem, /spaces/, "a tag name is git's to refuse, said before the click");
  assert.match(sourceBody({ kind: "tag", newTag: true, tagName: "v2" }).problem, /where the tag goes/);
  assert.deepEqual(sourceBody({ kind: "pr", pr: 12 }), { source: { source: "pull_request", number: 12 }, problem: null });
  assert.match(sourceBody({ kind: "pr", pr: null }).problem, /Pick a pull request/);
  assert.match(sourceBody({ kind: "pr", pr: 0 }).problem, /Pick a pull request/);
});

test("the branches other checkouts stand on are named with who holds them, the primary's included; a closed record holds nothing", () => {
  const taken = takenBranches(
    [
      { name: "Dark mode", kind: { kind: "worktree", branch: "feature/dark" }, state: { state: "open" } },
      { name: null, kind: { kind: "worktree", branch: "work/x-abc123" }, state: { state: "pr_open" } },
      { name: "Gone", kind: { kind: "worktree", branch: "feature/gone" }, state: { state: "closed" } },
      { name: null, kind: { kind: "primary" }, state: { state: "open" } },
    ],
    "main",
  );
  assert.deepEqual(
    [...taken.entries()],
    [
      ["main", "the primary"],
      ["feature/dark", "Dark mode"],
      ["work/x-abc123", "work/x-abc123"],
    ],
  );
  assert.equal(takenBranches([], null).size, 0);
});

test("a door's preset becomes that source's fields, and no preset is a new branch", () => {
  assert.deepEqual(fieldsOf(null), { kind: "new", name: "", start: "" });
  assert.deepEqual(fieldsOf({ kind: "new", start: "v1" }), { kind: "new", name: "", start: "v1" });
  assert.deepEqual(fieldsOf({ kind: "branch", branch: "topic" }), { kind: "branch", branch: "topic" });
  assert.deepEqual(fieldsOf({ kind: "tag", tag: "v1" }), { kind: "tag", tag: "v1", newTag: false });
  assert.deepEqual(fieldsOf({ kind: "remote", remote: "origin", branch: "x" }), { kind: "remote", remote: "origin", branch: "x" });
  assert.deepEqual(fieldsOf({ kind: "pr", pr: 3 }), { kind: "pr", pr: 3 });
});

test("the toast names what the workstream stands on", () => {
  assert.equal(openWords(null, ""), "Copied into a workstream.");
  assert.equal(openWords({ kind: "new" }, "work/x-abc123"), "Opened work/x-abc123.");
  assert.equal(openWords({ kind: "branch", branch: "topic" }, "topic"), "Opened topic.");
  assert.equal(openWords({ kind: "remote", remote: "origin", branch: "feature/x" }, "feature/x"), "Opened feature/x, tracking origin/feature/x.");
  assert.equal(openWords({ kind: "tag", tag: "v1" }, "from/v1"), "Opened from/v1 at v1.");
  assert.equal(openWords({ kind: "tag", newTag: true, tagName: "v2", tagAt: "main" }, "from/v2"), "Opened from/v2 at v2.");
  assert.equal(openWords({ kind: "pr", pr: 12 }, "feature/pr"), "Opened feature/pr from pull request #12 — the lifecycle picks up where it stands.");
});

test("the body of an opening is what the dialog shows, by name: a label only while its field shows, a base only when it is not the default, none for a pull request", () => {
  const form = (over = {}) => ({ git: true, label: "", fields: { kind: "new", name: "", start: "" }, base: "main", defaultBranch: "main", ...over });
  // Nothing typed: a derived branch at the default base.
  assert.deepEqual(openBody(form()), { body: { source: { source: "new_branch", name: null, start: null } }, problem: null });
  assert.deepEqual(openBody(form({ label: "  Dark mode ", fields: { kind: "new", name: " feature/dark ", start: "v1" }, base: " develop " })), {
    body: { label: "Dark mode", source: { source: "new_branch", name: "feature/dark", start: "v1" }, base: "develop" },
    problem: null,
  });
  // The label's field shows for a new branch and for a copy, and nowhere else.
  assert.equal(labelShown(true, { kind: "new" }), true);
  assert.equal(labelShown(false, { kind: "branch" }), true, "a copy is listed by its label whatever the fields last said");
  for (const kind of ["branch", "remote", "tag", "pr"]) assert.equal(labelShown(true, { kind }), false, kind);
  // A label typed under New branch does not ride along once another source is chosen.
  const switched = openBody(form({ label: "Dark mode", fields: { kind: "branch", branch: "topic" } }));
  assert.deepEqual(switched, { body: { source: { source: "local_branch", name: "topic" } }, problem: null });
  // A base of the person's travels; the default does not; a pull request brings its own.
  assert.deepEqual(openBody(form({ fields: { kind: "remote", remote: "origin", branch: "x" }, base: "release" })).body, { source: { source: "remote_branch", remote: "origin", name: "x" }, base: "release" });
  assert.deepEqual(openBody(form({ fields: { kind: "pr", pr: 12 }, base: "release" })).body, { source: { source: "pull_request", number: 12 } });
  assert.deepEqual(openBody(form({ fields: { kind: "tag", tag: "v1" }, base: "", defaultBranch: null })).body, { source: { source: "tag", name: "v1", branch: null, create_at: null } });
  // A copy: no repository, so no source and no base — the label alone, or nothing.
  assert.deepEqual(openBody(form({ git: false, label: " cart-total ", fields: { kind: "tag", tag: "v1" }, base: "release" })), { body: { label: "cart-total" }, problem: null });
  assert.deepEqual(openBody(form({ git: false })), { body: {}, problem: null });
  // The one problem is the source's, and then nothing is sent.
  const stopped = openBody(form({ fields: { kind: "branch" } }));
  assert.equal(stopped.body, null);
  assert.match(stopped.problem, /Pick a branch/);
  // The dialog sends this and builds nothing of its own.
  const dialog = readFileSync(new URL("./NewWorkstreamDialog.tsx", import.meta.url), "utf8");
  assert.ok(dialog.includes("api.openWorkstream(pid, request.body)"));
  assert.ok(dialog.includes("const request = openBody("));
});
