/**
 * Where a branch is on its way to the base, and the one legal act. Run with
 * `node --test desktop/src/views/_work/prLifecycleModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { FOLLOW_MS, IDLE_MS, STEPS, latestRead, lifecycle, pollInterval, prExists, prTitleFrom, readWords } from "./prLifecycleModel.mjs";
import { AGENT_REVIEW_MARK, reviewFacts } from "./reviewStepModel.mjs";

const GITHUB = { draft_prs: true, reviewers: true, labels: true, merge_strategies: ["merge", "squash"], check_runs: true, review_comments: true, review_threads: true, delete_branch: true };
const NONE = { draft_prs: false, reviewers: false, labels: false, merge_strategies: [], check_runs: false, review_comments: false, review_threads: false, delete_branch: false };
const pr = (over = {}) => ({ number: 12, url: "u", title: "t", state: "open", is_draft: false, mergeable: true, head: "work/x", head_sha: "abcdef1234", base: "main", ...over });
const passed = [{ name: "ci", status: "completed", conclusion: "success" }];
const failing = [{ name: "ci", status: "completed", conclusion: "failure" }, { name: "lint", status: "completed", conclusion: "success" }];
/** The reviews as the step reads them: none, an agent's, yours, somebody's verdict. */
const agentReview = { author: "you", state: "commented", body: `${AGENT_REVIEW_MARK}general-agent\n\nLooks good.` };
const yours = { author: "you", state: "commented", body: "Agreed." };
const review = (reviews = [], openComments = 0, caps = GITHUB) =>
  reviewFacts({ reviews, viewer: "you", comments: Array.from({ length: openComments }, () => ({ is_resolved: false })), caps });
const facts = (over = {}) => ({ isGit: true, state: "open", aheadOfBase: 0, upstream: null, ahead: 0, base: "main", pr: null, checks: null, review: review(), caps: GITHUB, ...over });
const status = (out) => Object.fromEntries(out.steps.map((s) => [s.id, s.status]));
const step = (out, id) => out.steps.find((s) => s.id === id);
const open = (over = {}) => lifecycle(facts({ state: "pr_open", aheadOfBase: 3, upstream: "origin/work/x", ahead: 0, pr: pr(), checks: passed, ...over }));
const reviewed = review([agentReview, yours]);
/** The one invariant every lifecycle keeps: one accent dot, and it is the act's step. */
const oneInHand = (out, why) => {
  const current = out.steps.filter((s) => s.status === "current").map((s) => s.id);
  assert.ok(current.length <= 1, `${why}: at most one step is current, got ${current.join(", ")}`);
  if (out.cta) assert.equal(out.current, out.cta.step, `${why}: the step in hand is the act's`);
  if (current.length === 1) assert.equal(current[0], out.current, `${why}: the current step is the one in hand`);
};

test("the steps are the seven, in order — review reads before merge, clean up last — and every code host has a Checks step", () => {
  assert.deepEqual(STEPS, ["commit", "push", "open_pr", "checks", "review", "merge", "cleanup"]);
  assert.ok(STEPS.indexOf("review") < STEPS.indexOf("merge"), "the review reads above the merge — the order of reading, not a gate");
  assert.deepEqual(lifecycle(facts()).steps.map((s) => s.id), [...STEPS]);
  assert.deepEqual(lifecycle(facts({ caps: NONE })).steps.map((s) => s.id), [...STEPS], "seven steps on every code host — the spine never changes length");
  assert.equal(lifecycle(facts({ caps: NONE })).steps.find((s) => s.id === "checks").note, "not reported by this code host");
  assert.deepEqual(lifecycle(facts({ isGit: false })), { steps: [], current: null, cta: null, note: "A copy has no branch — nothing here is published." });
  for (const s of lifecycle(facts()).steps) assert.deepEqual(s.cautions, [], "every step carries a cautions list");
});

test("facts first: a terminal commit the record never saw is still a commit, and pushing is the upstream's word", () => {
  const fresh = lifecycle(facts());
  assert.equal(status(fresh).commit, "current");
  assert.deepEqual(fresh.cta, { id: "commit", step: "commit", label: "Commit in Git › Changes", secondary: true });
  oneInHand(fresh, "nothing committed");

  const committedInAShell = lifecycle(facts({ state: "open", aheadOfBase: 2 }));
  assert.equal(status(committedInAShell).commit, "done", "the record says open; the branch says two commits");
  assert.equal(status(committedInAShell).push, "current");
  assert.deepEqual(committedInAShell.cta, { id: "open_pr", step: "push", label: "Push and open pull request", pushes: true }, "push-and-open sits under Push, where the person is");
  assert.equal(committedInAShell.current, "push");
  oneInHand(committedInAShell, "committed in a shell");

  const pushedRecordWithNewWork = lifecycle(facts({ state: "pushed", aheadOfBase: 3, upstream: "origin/work/x", ahead: 1 }));
  assert.equal(status(pushedRecordWithNewWork).push, "current", "a commit since the push is unpushed work again");
  assert.equal(step(pushedRecordWithNewWork, "push").note, "1 commit not pushed");
  oneInHand(pushedRecordWithNewWork, "new work after a push");

  const pushed = lifecycle(facts({ state: "pushed", aheadOfBase: 3, upstream: "origin/work/x", ahead: 0 }));
  assert.equal(status(pushed).push, "done");
  assert.equal(step(pushed, "push").note, "on origin/work/x", "a pushed branch names its upstream");
  assert.equal(status(pushed).open_pr, "current");
  assert.deepEqual(pushed.cta, { id: "open_pr", step: "open_pr", label: "Open pull request" });
  oneInHand(pushed, "pushed");
  const gitlab = lifecycle(facts({ state: "pushed", aheadOfBase: 3, upstream: "origin/work/x", ahead: 0, noun: "merge request" }));
  assert.deepEqual(gitlab.cta, { id: "open_pr", step: "open_pr", label: "Open merge request" }, "GitLab's noun");
  assert.equal(step(gitlab, "open_pr").label, "Open merge request");
  assert.equal(lifecycle(facts({ state: "open", aheadOfBase: 2, noun: "merge request" })).cta.label, "Push and open merge request");
});

test("with a pull request open the merge is the one act at once — the review optional, its surface under its row, never the panel's act", () => {
  const fresh = open();
  assert.deepEqual(status(fresh), { commit: "done", push: "done", open_pr: "done", checks: "done", review: "todo", merge: "current", cleanup: "todo" });
  assert.deepEqual(fresh.cta, { id: "merge", step: "merge", label: "Merge", cautions: [] }, "no review yet, and the merge is offered all the same");
  assert.equal(fresh.current, "merge");
  assert.equal(step(fresh, "review").note, "optional — an agent's, yours, or none");
  assert.equal(step(fresh, "merge").note, null, "no review is no caution");
  oneInHand(fresh, "open");

  const agentOnly = open({ review: review([agentReview]) });
  assert.equal(status(agentOnly).review, "done", "an agent's review alone is a review");
  assert.equal(step(agentOnly, "review").note, "reviewed by general-agent");
  assert.equal(agentOnly.cta.id, "merge");

  const yoursOnly = open({ review: review([yours]) });
  assert.equal(status(yoursOnly).review, "done", "your review alone is a review");
  assert.equal(step(yoursOnly, "review").note, "reviewed by you");
  assert.equal(yoursOnly.cta.id, "merge");

  const both = open({ review: reviewed });
  assert.equal(step(both, "review").note, "reviewed by general-agent and you");
  assert.deepEqual(both.cta, { id: "merge", step: "merge", label: "Merge", cautions: [] });

  const requested = open({ review: review([agentReview, yours, { author: "carol", state: "changes_requested", body: "no" }]) });
  assert.equal(status(requested).review, "done");
  assert.equal(step(requested, "review").note, "changes requested by @carol");
  assert.equal(status(requested).merge, "current", "a request for changes never blocks");
  assert.deepEqual(requested.cta, { id: "merge", step: "merge", label: "Merge", cautions: ["changes requested by @carol"] }, "it is the caution the merge names");
  assert.equal(step(requested, "merge").note, "changes requested by @carol");
  for (const s of fresh.steps) assert.notEqual(s.status === "current" && s.id === "review", true, "the review is never current");
});

test("checks still running are waiting, never the step in hand: the merge stays the act, under Merge, with the caution", () => {
  const running = [{ name: "ci", status: "in_progress", conclusion: null }];
  const stillChecking = open({ checks: running });
  assert.equal(status(stillChecking).checks, "waiting", "in flight, nobody's act");
  assert.equal(status(stillChecking).merge, "current");
  assert.equal(stillChecking.current, "merge", "the step in hand is the act's — never Checks");
  assert.deepEqual(stillChecking.cta, { id: "merge", step: "merge", label: "Merge", cautions: ["checks still running"] }, "legal, with the caution");
  assert.equal(step(stillChecking, "merge").note, "checks still running", "the Merge row wears it");
  assert.deepEqual(step(stillChecking, "review").cautions, [], "the Review row does not: it is the checks', not the review's");
  oneInHand(stillChecking, "checks running");

  const withComments = open({ checks: running, review: review([agentReview], 2) });
  assert.deepEqual(withComments.cta.cautions, ["checks still running", "2 comments open"], "the checks lead the cautions");

  const reading = open({ checks: null });
  assert.equal(status(reading).checks, "waiting");
  assert.equal(step(reading, "checks").note, "reading the code host…");
  assert.equal(reading.current, "merge");
  assert.deepEqual(reading.cta.cautions, [], "nothing read yet is nothing to caution on");
  oneInHand(reading, "reading the code host");

  const failingNow = open({ checks: failing });
  assert.equal(status(failingNow).checks, "blocked");
  assert.equal(status(failingNow).merge, "blocked");
  assert.equal(failingNow.current, "merge", "the act is the merge's, blocked, and the button sits under Merge");
  assert.deepEqual(failingNow.cta, { id: "merge", step: "merge", label: "Merge", blocked: "1 passed · 1 failed", cautions: [] });
  oneInHand(failingNow, "checks failing");

  assert.equal(status(open({ checks: running, caps: { ...GITHUB, check_runs: false } })).checks, "done", "a code host that reports no checks cannot be waiting on them");
  assert.deepEqual(open({ checks: running, caps: { ...GITHUB, check_runs: false } }).cta.cautions, []);

  // A code host that reports check runs and lists none for this pull request: nothing runs, so nothing waits — read from the list, never from the summary's words.
  const none = open({ checks: [] });
  assert.equal(status(none).checks, "done");
  assert.equal(step(none, "checks").note, "no checks");
  assert.deepEqual(none.cta.cautions, [], "no run is no caution");
});

test("the merge is blocked, with its reason, only by what the code host itself cannot merge — reviewed or not; a host still computing is waited on", () => {
  const approved = open({ review: review([{ author: "alice", state: "approved", body: "" }]) });
  assert.equal(status(approved).review, "done");
  assert.equal(step(approved, "review").note, "approved by @alice");
  assert.equal(approved.cta.id, "merge");

  assert.equal(open({ checks: failing }).cta.blocked, "1 passed · 1 failed");
  assert.equal(status(open({ checks: failing })).checks, "blocked");
  assert.equal(open({ pr: pr({ mergeable: false }) }).cta.blocked, "conflicts with main");
  assert.equal(status(open({ pr: pr({ mergeable: false }) })).merge, "blocked");
  assert.equal(open({ pr: pr({ is_draft: true }) }).cta.blocked, "a draft — mark it ready on the code host");
  assert.equal(open({ checks: failing, caps: { ...GITHUB, check_runs: false } }).cta.blocked, undefined, "nor on checks it does not report");
  assert.equal(open({ review: reviewed, checks: failing }).cta.blocked, "1 passed · 1 failed", "a review does not lift a block");

  const computing = open({ pr: pr({ mergeable: null }) });
  assert.equal(computing.cta.blocked, "the code host is still checking");
  assert.equal(status(computing).merge, "waiting", "nothing is wrong: the host is not done");
  assert.equal(step(computing, "merge").note, "the code host is still checking");
  assert.equal(computing.current, "merge", "the act holds under Merge, off, with the reason");
  oneInHand(computing, "mergeability computing");
  // Waiting is the fact of a host still computing, not the shape of a sentence: what the host itself cannot merge blocks even while it computes.
  const draftComputing = open({ pr: pr({ mergeable: null, is_draft: true }) });
  assert.equal(status(draftComputing).merge, "blocked", "a draft blocks whatever the host is still computing");
  assert.equal(draftComputing.cta.blocked, "a draft — mark it ready on the code host");
  const closedComputing = open({ pr: pr({ mergeable: null, state: "closed" }) });
  assert.equal(status(closedComputing).merge, "blocked");

  const unpushed = open({ ahead: 2 });
  assert.equal(status(unpushed).push, "current");
  assert.deepEqual(unpushed.cta, { id: "push", step: "push", label: "Push in Git › Changes", secondary: true }, "new commits go out before the merge is offered");
  assert.equal(unpushed.current, "push");
  assert.equal(status(unpushed).merge, "blocked", "the merge says why it is off");
  oneInHand(unpushed, "commits not pushed");

  const closedOnHost = open({ pr: pr({ state: "closed" }) });
  assert.equal(status(closedOnHost).open_pr, "done", "the pull request was opened: one mark for the one fact, on the Merge row");
  assert.equal(status(closedOnHost).merge, "blocked", "the merge is off for good");
  assert.equal(step(closedOnHost, "merge").note, "closed on the code host");
  assert.equal(closedOnHost.cta.id, "close", "the act is the way out, under Clean up — never a merge button under Open pull request");
  assert.equal(closedOnHost.current, "cleanup");
  assert.equal(status(closedOnHost).review, "todo", "nothing to review on a closed pull request");
  oneInHand(closedOnHost, "closed on the code host");
});

test("merged on the code host before the record heard: Clean up is the act, every earlier step done, commits after the merge its caution and never the act, and the merge commit is what it merged as", () => {
  const web = lifecycle(facts({ state: "pr_open", aheadOfBase: 3, upstream: "origin/work/x", ahead: 2, pr: pr({ state: "merged", merge_commit: "fedcba9876" }), checks: passed, review: review([], 2) }));
  assert.deepEqual(status(web), { commit: "done", push: "done", open_pr: "done", checks: "done", review: "done", merge: "done", cleanup: "current" });
  assert.deepEqual(web.cta, { id: "cleanup", step: "cleanup", label: "Clean up branch", cautions: ["2 commits never reached the pull request — they go with the branch"] });
  assert.equal(step(web, "cleanup").note, "2 commits never reached the pull request — they go with the branch");
  assert.equal(web.current, "cleanup", "the record still says pr_open: the host's word wins");
  assert.equal(step(web, "merge").note, "as fedcba9", "the merge commit, not the branch tip");
  assert.deepEqual(step(web, "review").cautions, [], "nothing left to caution about");
  oneInHand(web, "merged on the web");
  const tip = lifecycle(facts({ state: "pr_open", aheadOfBase: 3, upstream: "origin/work/x", ahead: 0, pr: pr({ state: "merged" }), checks: passed }));
  assert.equal(step(tip, "merge").note, "as abcdef1", "a host that names no merge commit: the tip");
  assert.deepEqual(tip.cta, { id: "cleanup", step: "cleanup", label: "Clean up branch" }, "nothing stranded: no caution");
  const one = lifecycle(facts({ state: "merged", aheadOfBase: 3, upstream: "origin/work/x", ahead: 1, pr: pr({ state: "merged" }) }));
  assert.deepEqual(one.cta.cautions, ["1 commit never reached the pull request — they go with the branch"]);
  assert.equal(status(one).push, "done", "nothing left to push on a merged branch");
});

test("a pull request closed on the code host without merging ends in the way out: the merge off, the push settled, Close this workstream under Clean up", () => {
  const out = open({ pr: pr({ state: "closed" }), ahead: 2 });
  assert.deepEqual(status(out), { commit: "done", push: "done", open_pr: "done", checks: "done", review: "todo", merge: "blocked", cleanup: "current" });
  assert.deepEqual(out.cta, { id: "close", step: "cleanup", label: "Close this workstream…" });
  assert.equal(step(out, "open_pr").note, "#12", "opened, whatever the host did since");
  assert.equal(step(out, "merge").note, "closed on the code host", "the one danger mark");
  assert.equal(step(out, "cleanup").note, "the pull request was closed without merging — close this workstream, keeping or removing its checkout");
  oneInHand(out, "closed without merging");
});

test("a code host that could not be read is said, not guessed: the checks wait with the reason and the merge is cautioned, a stale block stands, and a merge is never a live button for a request nobody has read", () => {
  const unread = open({ checks: null, checksError: "boom" });
  assert.equal(status(unread).checks, "waiting");
  assert.equal(step(unread, "checks").note, "the checks could not be read", "the why is the error note's, under the row");
  assert.equal(status(unread).merge, "current", "the code host decides at the merge");
  assert.deepEqual(unread.cta.cautions, ["checks could not be read; the code host decides"]);
  oneInHand(unread, "checks unread");
  assert.equal(open({ checks: null }).cta.cautions.length, 0, "not read yet is not a failed read");
  assert.equal(step(open({ checks: null }), "checks").note, "reading the code host…");
  const stale = open({ checks: failing, checksError: "boom" });
  assert.equal(stale.cta.blocked, "1 passed · 1 failed", "a block read before the read failed still stands");
  const noPr = open({ pr: null, prError: "boom" });
  assert.equal(status(noPr).merge, "waiting");
  assert.equal(noPr.cta.blocked, "the pull request could not be read");
  assert.equal(step(noPr, "merge").note, "the pull request could not be read");
  oneInHand(noPr, "pull request unread");
  const reading = open({ pr: null });
  assert.equal(reading.cta.blocked, "reading the code host…", "never a live generic button");
  assert.equal(status(reading).merge, "waiting");
});

test("the host's own word on the merge: blocked blocks with the failing checks or its reason, dirty is conflicts, draft a draft, behind and unstable are cautions, clean merges past a failure nothing requires, and a host without the word keeps the rule", () => {
  assert.equal(open({ pr: pr({ merge_state: "blocked" }), checks: failing }).cta.blocked, "1 passed · 1 failed");
  assert.equal(open({ pr: pr({ merge_state: "blocked" }), checks: passed }).cta.blocked, "the code host blocks the merge (required reviews or checks)");
  assert.equal(open({ pr: pr({ merge_state: "dirty", mergeable: true }) }).cta.blocked, "conflicts with main");
  assert.equal(open({ pr: pr({ merge_state: "draft" }) }).cta.blocked, "a draft — mark it ready on the code host");
  const behind = open({ pr: pr({ merge_state: "behind" }) });
  assert.equal(status(behind).merge, "current");
  assert.deepEqual(behind.cta.cautions, ["behind main — update the branch"]);
  const unstable = open({ pr: pr({ merge_state: "unstable" }), checks: failing });
  assert.equal(status(unstable).merge, "current", "the host can merge: a failing check it does not require is a caution");
  assert.deepEqual(unstable.cta.cautions, ["1 check failing, none required"]);
  assert.equal(status(unstable).checks, "done");
  assert.deepEqual(step(unstable, "checks").cautions, ["1 passed · 1 failed"], "the Checks row wears it");
  assert.equal(step(unstable, "checks").note, "1 passed · 1 failed");
  oneInHand(unstable, "unstable");
  const clean = open({ pr: pr({ merge_state: "clean", mergeable: null }), checks: failing });
  assert.equal(status(clean).merge, "current", "clean past a failure, and a host that said clean has answered");
  assert.equal(clean.cta.blocked, undefined);
  assert.deepEqual(clean.cta.cautions, [], "clean names no failing check of its own");
  assert.equal(open({ pr: pr({ merge_state: "unknown" }), checks: failing }).cta.blocked, "1 passed · 1 failed", "unknown is today's rule");
  assert.equal(open({ pr: pr({ merge_state: "unknown", mergeable: null }) }).cta.blocked, "the code host is still checking");
  const stale = open({ checks: [{ name: "ci", status: "completed", conclusion: "stale" }] });
  assert.equal(status(stale).checks, "done");
  assert.deepEqual(step(stale, "checks").cautions, ["1 in an unknown state"], "a run in a state nobody knows is a caution, never a pass");
  assert.equal(status(stale).merge, "current");
});

test("comments still open are a caution the merge names, never a block; a code host that exposes none cannot caution", () => {
  const comments = open({ review: review([agentReview, yours], 3) });
  assert.equal(status(comments).review, "done");
  assert.equal(status(comments).merge, "current");
  assert.deepEqual(comments.cta, { id: "merge", step: "merge", label: "Merge", cautions: ["3 comments open"] });
  assert.equal(step(comments, "merge").note, "3 comments open", "the merge row wears the caution");
  assert.deepEqual(step(comments, "review").cautions, ["3 comments open"]);

  const one = open({ review: review([{ author: "alice", state: "approved", body: "" }], 1) });
  assert.deepEqual(one.cta.cautions, ["1 comment open"]);

  assert.deepEqual(open({ review: review([agentReview, yours], 3, { ...GITHUB, review_threads: false }), caps: { ...GITHUB, review_threads: false } }).cta.cautions, [], "a code host that exposes no comments cannot caution on them");

  const blockedAndCautioned = open({ review: review([agentReview, yours], 2), checks: failing });
  assert.equal(blockedAndCautioned.cta.blocked, "1 passed · 1 failed", "a block is still a block");
  assert.deepEqual(blockedAndCautioned.cta.cautions, ["2 comments open"], "and the caution rides along for when it lifts");

  const pending = open({ review: review([], 2) });
  assert.deepEqual(step(pending, "review").cautions, ["2 comments open"], "the review row names them too");
  assert.equal(status(pending).merge, "current", "comments with no review: still a caution, still legal");
  assert.deepEqual(pending.cta.cautions, ["2 comments open"]);

  const bothCautions = open({ review: review([{ author: "carol", state: "changes_requested", body: "no" }], 1) });
  assert.deepEqual(bothCautions.cta.cautions, ["1 comment open", "changes requested by @carol"], "comments first, then the request for changes");
});

test("a merged branch keeps its pull request and offers Clean up; a closed workstream offers nothing", () => {
  const merged = lifecycle(facts({ state: "merged", aheadOfBase: 3, upstream: "origin/work/x", ahead: 0, pr: pr({ state: "merged" }), checks: passed, review: review([], 2) }));
  assert.deepEqual(status(merged), { commit: "done", push: "done", open_pr: "done", checks: "done", review: "done", merge: "done", cleanup: "current" });
  assert.deepEqual(merged.cta, { id: "cleanup", step: "cleanup", label: "Clean up branch" });
  assert.equal(merged.current, "cleanup");
  assert.equal(step(merged, "merge").note, "as abcdef1");
  assert.deepEqual(step(merged, "review").cautions, [], "a merged pull request has nothing left to caution about");
  oneInHand(merged, "merged");
  const closed = lifecycle(facts({ state: "closed", aheadOfBase: 3, upstream: "o", ahead: 0, pr: pr({ state: "merged" }) }));
  assert.equal(closed.cta, null);
  assert.equal(closed.current, null, "nothing in hand on a closed workstream");
  assert.equal(closed.note, "This workstream is closed.");
  assert.equal(status(closed).cleanup, "done");
  oneInHand(closed, "closed");
});

test("the line under the spine says when the code host was read, and that an agent is being followed", () => {
  assert.equal(readWords(null, false, null), null, "nothing read, nobody followed: no line");
  assert.equal(readWords(null, true, "fixer"), "following fixer · reading the code host…");
  assert.equal(readWords(1_000, false, null, 1_002), "code host read just now");
  assert.equal(readWords(1_000, false, null, 1_012), "code host read 12 s ago");
  assert.equal(readWords(1_000, false, null, 1_200), "code host read 3 min ago");
  assert.equal(readWords(1_000, false, null, 8_300), "code host read 2 h ago");
  assert.equal(readWords(1_000, true, "general-agent", 1_005), "following general-agent · read 5 s ago");
  assert.equal(readWords(1_000, true, null, 1_005), "following the agent · read 5 s ago", "no name is still a follow");
  assert.equal(readWords(1_010, false, null, 1_000), "code host read just now", "a clock behind the read never says a negative");
});

test("a pull request is titled by the branch tip's subject, else the branch", () => {
  assert.equal(prTitleFrom("Fix the cart total rounding\n\nLonger body", "work/cart"), "Fix the cart total rounding");
  assert.equal(prTitleFrom("  ", "work/cart"), "work/cart");
  assert.equal(prTitleFrom(null, null), "");
});

test("the code host is asked only once the record names a pull request; the poll follows an agent first, the fetch interval next, and idles when nothing is to be read", () => {
  assert.ok(prExists("pr_open") && prExists("merged"));
  for (const word of ["open", "dirty", "committed", "pushed", "closed", null, undefined]) assert.ok(!prExists(word), String(word));
  assert.equal(pollInterval({ hasPr: true, following: true, secs: 300 }), FOLLOW_MS, "following wins over the interval");
  assert.equal(pollInterval({ hasPr: true, following: true, secs: 0 }), FOLLOW_MS, "polling off does not turn the following off");
  assert.equal(pollInterval({ hasPr: true, following: false, secs: 300 }), 300_000);
  assert.equal(pollInterval({ hasPr: true, following: false, secs: 0 }), IDLE_MS, "0 turns the polling off");
  assert.equal(pollInterval({ hasPr: false, following: true, secs: 300 }), IDLE_MS, "no pull request: nothing to follow or poll");
  assert.ok(FOLLOW_MS > 10_000 && FOLLOW_MS < 60_000, "above the engine's ten-second cache, below a person's patience");
});

test("the read time under the spine is the latest of the three reads, nulls ignored, and none before any", () => {
  assert.equal(latestRead([null, 10, 7]), 10);
  assert.equal(latestRead([5, null, 9]), 9);
  assert.equal(latestRead([null, null]), null);
  assert.equal(latestRead([]), null);
  assert.equal(latestRead(null), null);
  assert.equal(latestRead([Number.NaN, 3]), 3, "a time nobody can read is no time");
});
