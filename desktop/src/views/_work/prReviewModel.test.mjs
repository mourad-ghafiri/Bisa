/**
 * PR review facts. Run with
 * `node --test desktop/src/views/_work/prReviewModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { checkFixPrompt, commentsByFile, failedCheck, fixPrompt, reviewStateLabel, reviewTone, unresolvedCount } from "./prReviewModel.mjs";

const th = (id, path, line, resolved, comments = [{ body: "fix this" }]) => ({
  id,
  path,
  line,
  is_resolved: resolved,
  is_outdated: false,
  comments,
});

test("fixPrompt lists the open comments by place with their thread ids, in the host's noun, tells the agent to reply and resolve on the thread, and forbids a push or a merge", () => {
  const p = fixPrompt({ number: 7 }, [th("T1", "src/a.rs", 12, false, [{ body: "rounds twice" }]), th("T2", "x", null, true, [{ body: "done" }])], "merge request");
  assert.match(p, /comments on merge request #7/);
  assert.match(p, /- src\/a\.rs:12 \[thread T1\]: rounds twice/);
  assert.doesNotMatch(p, /done/, "a resolved comment is not listed");
  assert.match(p, /Do not push or merge/);
  assert.match(p, /`pr_thread_reply`/, "the agent answers on the thread itself");
  assert.match(p, /`resolve: true`/, "and resolves what it addressed");
  assert.match(p, /Leave a comment you did not address open/);
});

test("fixPrompt handed one comment lists that one alone — the row's Fix", () => {
  const one = th("T9", "src/b.rs", 3, false, [{ body: "rename this" }, { body: "agreed" }]);
  const p = fixPrompt({ number: 7 }, [one]);
  assert.match(p, /pull request #7/);
  assert.match(p, /- src\/b\.rs:3 \[thread T9\]: rename this \/ agreed/);
  assert.equal((p.match(/^- /gm) ?? []).length, 1);
});

test("checkFixPrompt names the failed check, its conclusion, the host's summary and the log, and bounds the fix to this branch", () => {
  const p = checkFixPrompt({ number: 7 }, { name: "ci / lint", conclusion: "timed_out", summary: "2 warnings", url: "https://ci.example/run/1" }, "merge request");
  assert.match(p, /^Check "ci \/ lint" failed \(timed out\) on merge request #7\./);
  assert.match(p, /The code host says: 2 warnings/);
  assert.match(p, /Its log: https:\/\/ci\.example\/run\/1/);
  assert.match(p, /Reproduce the failure in this checkout first/);
  assert.match(p, /Do not push or merge/);
  assert.match(p, /Reply here with what failed and what you changed\./);
  assert.doesNotMatch(p, /pr_review_submit|pr_thread/, "a check has no thread to answer on");
  const bare = checkFixPrompt({ number: 1 }, { name: "build", conclusion: null, summary: null, url: null });
  assert.match(bare, /^Check "build" failed on pull request #1\./);
  assert.doesNotMatch(bare, /code host says|Its log/, "nothing invented for what the host did not say");
});

test("a check failed when it completed as a failure, a timeout, an action required or a cancellation — never while running", () => {
  for (const conclusion of ["failure", "timed_out", "action_required", "cancelled"]) assert.ok(failedCheck({ status: "completed", conclusion }), conclusion);
  for (const conclusion of ["success", "neutral", "skipped", null]) assert.ok(!failedCheck({ status: "completed", conclusion }), String(conclusion));
  assert.ok(!failedCheck({ status: "in_progress", conclusion: "failure" }), "a run still going has not failed yet");
  assert.ok(!failedCheck({ status: "queued", conclusion: null }));
});

test("unresolvedCount counts only open comments", () => {
  assert.equal(unresolvedCount([th("a", "x", 1, false), th("b", "x", 2, true)]), 1);
  assert.equal(unresolvedCount([]), 0);
});

test("commentsByFile groups by file, files with open comments first, comments by line", () => {
  const groups = commentsByFile([th("1", "b.rs", 5, true), th("2", "a.rs", 20, false), th("3", "a.rs", 4, true), th("4", null, 1, false)]);
  assert.deepEqual(groups.map((g) => g.path), ["", "a.rs", "b.rs"], "the general one leads, then the file with open comments");
  assert.deepEqual(groups[1].comments.map((t) => t.id), ["3", "2"], "by line within a file");
  assert.equal(groups[1].unresolved, 1);
  assert.equal(groups[2].unresolved, 0);
});

test("a verdict's word and tone", () => {
  assert.equal(reviewStateLabel("approved"), "Approved");
  assert.equal(reviewStateLabel("changes_requested"), "Changes requested");
  assert.equal(reviewStateLabel("commented"), "Commented");
  assert.equal(reviewStateLabel("dismissed"), "Dismissed");
  assert.equal(reviewStateLabel("pending"), "Pending");
  assert.equal(reviewStateLabel("other"), "other");
  assert.deepEqual(["approved", "changes_requested", "dismissed", "commented"].map(reviewTone), ["ok", "danger", "quiet", "neutral"]);
});
