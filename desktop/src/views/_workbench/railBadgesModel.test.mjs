import test from "node:test";
import assert from "node:assert/strict";
import { badgeTooltip, gitBadge, workstreamsBadge } from "./railBadgesModel.mjs";

const clean = { git: true, exists: true, staged: 0, unstaged: 0, untracked: 0, conflicted: 0 };

test("the Git mark counts what there is to commit or discard, and conflicts outrank it", () => {
  assert.equal(gitBadge(null), null);
  assert.equal(gitBadge(clean), null);
  assert.equal(gitBadge({ ...clean, git: false, untracked: 3 }), null, "a plain folder has nothing to commit");
  assert.deepEqual(gitBadge({ ...clean, untracked: 1 }), { tone: "accent", title: "1 change to commit or discard" });
  assert.deepEqual(gitBadge({ ...clean, staged: 2, unstaged: 1, untracked: 1 }), { tone: "accent", title: "4 changes to commit or discard" });
  assert.deepEqual(gitBadge({ ...clean, staged: 2, conflicted: 1 }), { tone: "danger", title: "1 conflict to resolve" });
});

const review = (kind, over = {}) => ({ run: { kind, agent: "reviewer" }, starting: false, live: true, done: false, ...over });

test("the Workstreams mark is a branch's alone: an agent at work, a git act in flight, then a pull request open", () => {
  const branch = { kind: "worktree", pr: null, busy: null, run: null };
  assert.equal(workstreamsBadge(branch), null, "nothing is happening to a plain branch");
  assert.equal(workstreamsBadge({ ...branch, kind: "primary", pr: { number: 3 } }), null, "the primary has no lifecycle");
  assert.equal(workstreamsBadge({ ...branch, kind: "copy", pr: { number: 3 } }), null);
  assert.deepEqual(workstreamsBadge({ ...branch, pr: { number: 12 } }), { tone: "accent", title: "pull request #12 open — checks, review and merge in hand" });
  assert.deepEqual(workstreamsBadge({ ...branch, pr: { number: 12 }, busy: "merge" }), { tone: "working", title: "merging…" });
  assert.deepEqual(workstreamsBadge({ ...branch, busy: "pr" }), { tone: "working", title: "opening the pull request…" });
  assert.deepEqual(workstreamsBadge({ ...branch, busy: "push" }), { tone: "working", title: "pushing…" });
  assert.equal(workstreamsBadge({ ...branch, busy: "stash" }), null, "an act that is not the lifecycle's is not its mark");
  assert.deepEqual(workstreamsBadge({ ...branch, pr: { number: 12 }, busy: "merge", run: review("review") }), { tone: "working", title: "reviewer is reviewing" });
  assert.deepEqual(workstreamsBadge({ ...branch, run: review("fix", { live: false, starting: true }) }), { tone: "working", title: "reviewer is fixing" });
  assert.equal(workstreamsBadge({ ...branch, run: review("review", { live: false, done: true }) }), null, "an ended run is not in progress");
});

test("the tooltip carries the mark's sentence after the tab's name", () => {
  assert.equal(badgeTooltip("Git · ⌘⇧G", false, null), "Show Git · ⌘⇧G");
  assert.equal(badgeTooltip("Git", true, { tone: "accent", title: "2 changes to commit or discard" }), "Hide Git · 2 changes to commit or discard");
});
