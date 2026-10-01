/**
 * A workstream from its opening to its close (ide/07, ide/08): a project
 * takes one once it has a commit, the dialog previews the branch the engine
 * settles on and makes the wire body, the branch panel offers each branch the
 * verbs it can take, the pull-request lifecycle walks its seven steps with
 * one in hand, and a close counts what it terminates and says so. No DOM.
 *
 * Run with `node --test desktop/src/scenarios/workstreamLifecycle.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { TAIL_PLACEHOLDER, canOpenWorkstream, previewBranch, sourceBody } from "../views/_work/workstreamCreation.mjs";
import { branchActions, menuActions } from "../views/_work/branchActionsModel.mjs";
import { STEPS, lifecycle } from "../views/_work/prLifecycleModel.mjs";
import { reviewFacts } from "../views/_work/reviewStepModel.mjs";
import { terminationConsent, terminationCounts, terminationWords } from "../views/_work/closeWorkstreamModel.mjs";

const GITHUB = { draft_prs: true, reviewers: true, labels: true, merge_strategies: ["merge", "squash"], check_runs: true, review_comments: true, review_threads: true, delete_branch: true };
const pr = (over = {}) => ({ number: 12, url: "u", title: "t", state: "open", is_draft: false, mergeable: true, head: "work/dark-mode", head_sha: "abcdef1234", base: "main", ...over });
const facts = (over = {}) => ({ isGit: true, state: "open", aheadOfBase: 0, upstream: null, ahead: 0, base: "main", pr: null, checks: null, review: reviewFacts({ reviews: [], viewer: "you", comments: [], caps: GITHUB }), caps: GITHUB, ...over });
const status = (out) => Object.fromEntries(out.steps.map((s) => [s.id, s.status]));

test("opening: a project without a commit refuses with a remedy, then the dialog previews the branch and makes the body for every source", () => {
  assert.equal(canOpenWorkstream({ exists: true }, { git: true, exists: true, head: null }).reason, "unborn");
  assert.deepEqual(canOpenWorkstream({ exists: true }, { git: true, exists: true, head: "abc" }), { ok: true, mode: "branch" });
  // A label alone: the engine derives the branch and adds its tail.
  assert.deepEqual(previewBranch({ label: "Dark mode" }), { branch: `work/dark-mode-${TAIL_PLACEHOLDER}`, derived: true });
  // A typed name is made safe, never refused, and is the person's.
  assert.deepEqual(previewBranch({ label: "x", source: { kind: "new", name: "Feature/Dark Mode" } }), { branch: "feature/dark-mode", derived: false });
  assert.deepEqual(sourceBody({ kind: "new", name: " feature/dark-mode ", start: "v1" }), { source: { source: "new_branch", name: "feature/dark-mode", start: "v1" }, problem: null });
  // An existing branch, a remote's, a tag, a pull request: each its own wire word.
  assert.deepEqual(previewBranch({ source: { kind: "branch", branch: "topic" } }), { branch: "topic", derived: false });
  assert.deepEqual(previewBranch({ source: { kind: "remote", remote: "origin", branch: "feature/x" } }), { branch: "feature/x", derived: false });
  assert.deepEqual(previewBranch({ source: { kind: "tag", tag: "v1.2.0" } }), { branch: "from/v1.2.0", derived: false });
  assert.deepEqual(previewBranch({ source: { kind: "pr", pr: 12, head: "feature/pr" } }), { branch: "feature/pr", derived: false });
  for (const source of [{ kind: "branch", branch: "topic" }, { kind: "remote", remote: "origin", branch: "feature/x" }, { kind: "tag", tag: "v1.2.0" }, { kind: "pr", pr: 12, head: "feature/pr" }]) {
    const body = sourceBody(source);
    assert.equal(body.problem, null, `${source.kind} makes a body`);
    assert.equal(typeof body.source.source, "string");
  }
});

test("working: the branch panel offers the verbs each branch can take, the default never renamed or deleted, the current one rebased and pushed with a lease", () => {
  const ctx = { current: "work/dark-mode", defaultBranch: "main" };
  const other = branchActions({ name: "topic", current: false }, ctx).map((a) => a.id);
  assert.deepEqual(other, ["switch", "merge", "rebase", "cherry_pick", "upstream", "workstream", "rename", "delete"]);
  assert.deepEqual(menuActions({ name: "topic", current: false }, ctx).map((a) => a.id), other.filter((id) => id !== "switch"), "the menu is the row's verbs without the hover's Switch");
  const main = branchActions({ name: "main", current: false }, ctx).map((a) => a.id);
  assert.ok(!main.includes("rename") && !main.includes("delete"), "the default branch is never renamed or deleted");
  const current = branchActions({ name: "work/dark-mode", current: true }, ctx).map((a) => a.id);
  assert.ok(!current.includes("switch"), "the current branch is not switched to");
  assert.ok(!current.includes("delete"), "nor deleted from under you");
});

test("publishing: the seven steps walk with one in hand — commit, push, open, checks, review, merge, clean up", () => {
  assert.deepEqual(STEPS, ["commit", "push", "open_pr", "checks", "review", "merge", "cleanup"]);
  // Fresh: nothing committed yet, the commit is in hand.
  let out = lifecycle(facts());
  assert.equal(out.current, "commit");
  // Committed ahead of the base, no upstream: the push is in hand.
  out = lifecycle(facts({ state: "committed", aheadOfBase: 2 }));
  assert.equal(out.current, "push");
  assert.equal(status(out).commit, "done");
  // Pushed: open the pull request.
  out = lifecycle(facts({ state: "pushed", aheadOfBase: 2, upstream: "origin/work/dark-mode" }));
  assert.equal(out.current, "open_pr");
  // Open with passing checks and no review: the review reads, the merge is offered whenever the pull request is open.
  out = lifecycle(facts({ state: "pr_open", aheadOfBase: 2, upstream: "origin/work/dark-mode", pr: pr(), checks: [{ name: "ci", status: "completed", conclusion: "success" }] }));
  assert.equal(status(out).checks, "done");
  assert.ok(["review", "merge"].includes(out.current), `after the checks the review reads or the merge is in hand: ${out.current}`);
  assert.ok(out.cta === null || STEPS.includes(out.cta.step), "the act names a step");
  // Merged: clean up.
  out = lifecycle(facts({ state: "merged", aheadOfBase: 0, upstream: "origin/work/dark-mode", pr: pr({ state: "merged" }) }));
  assert.equal(status(out).merge, "done");
  assert.equal(out.current, "cleanup");
  // A copy has no branch: nothing here is published.
  assert.deepEqual(lifecycle(facts({ isGit: false })).steps, []);
});

test("closing: a close counts the live harness tabs, the shells and the engine's sessions rooted here — exited tabs and ended sessions apart — and asks in those words", () => {
  const tab = (id, harness, status = "live") => ({ scope: "workstream", id, harness, liveness: { status, code: null } });
  const row = (workstream, kind, state) => ({ workstream, kind, state });
  const terminals = [tab("w1", "claude-code"), tab("w1", null), tab("w1", "codex", "exited"), tab("w2", null)];
  const sessions = [row("w1", "worker", "running"), row("w1", "worker", "done"), row("w1", "terminal", "running"), row("w2", "worker", "idle")];
  const counts = terminationCounts(sessions, terminals, "w1");
  assert.deepEqual(counts, { harnesses: 1, shells: 1, agents: 1 });
  assert.equal(terminationWords(counts), "1 harness terminated, 1 shell closed and 1 agent session aborted");
  assert.equal(terminationConsent(counts), "Closing it ends what stands in it: 1 harness terminated, 1 shell closed and 1 agent session aborted.");
  assert.equal(terminationConsent(terminationCounts([], [tab("w1", "codex", "exited")], "w1")), null, "nothing live: nothing to consent to");
});
