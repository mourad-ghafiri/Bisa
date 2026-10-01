/**
 * Asking an agent to review, and the person's own review: the two prompts,
 * the person's words leading, the buttons per allowed verdict, the reasons.
 * Run with `node --test desktop/src/views/_work/agentReviewModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { EVENTS } from "./reviewStepModel.mjs";
import { DEFAULT_AGENT, FIX_MENU_LABEL, askContent, askLabel, branchReviewPrompt, draftKeys, fixAllLabel, fixLabel, landsWords, reviewButtons, reviewPrompt, wordsReason } from "./agentReviewModel.mjs";

test("the pull-request prompt asks for a comment with the verdict in its words, never an approval the code host would refuse", () => {
  const p = reviewPrompt({ number: 7, title: "Dark mode" });
  assert.match(p, /pull request #7 — "Dark mode"/);
  assert.match(p, /`event: comment`/);
  assert.match(p, /refuses an `approve` or `request_changes`/);
  assert.match(p, /pr_review_submit/);
  assert.match(p, /signs the review with your agent id — do not sign it yourself/, "the engine signs; a second signature would confuse the reader");
  assert.match(reviewPrompt({ number: 7, title: "Dark mode" }, "merge request"), /^Please review merge request #7/, "GitLab's noun");
});

test("the branch prompt names the base, reads the diff in the checkout, replies in the thread and changes nothing", () => {
  const p = branchReviewPrompt("main");
  assert.match(p, /^Please review this workstream's branch against main\./);
  assert.match(p, /`git diff main\.\.\.HEAD`/);
  assert.match(p, /reply here with your review — there is no pull request/);
  assert.match(p, /Change nothing: no edits, no commits, no push, no pull request/);
  assert.doesNotMatch(p, /pr_review_submit/, "no code host to post on");
});

test("the person's words lead either prompt, a paragraph apart, and blank words add nothing", () => {
  const withWords = reviewPrompt({ number: 7, title: "Dark mode" }, "pull request", "Look at the rounding in total().");
  assert.ok(withWords.startsWith("Look at the rounding in total().\n\nPlease review pull request #7"), withWords);
  assert.equal(reviewPrompt({ number: 7, title: "Dark mode" }, "pull request", "   "), reviewPrompt({ number: 7, title: "Dark mode" }));
  assert.ok(branchReviewPrompt("main", "Focus on the migration.").startsWith("Focus on the migration.\n\nPlease review"));
  assert.equal(askContent({ kind: "pr", pr: { number: 7, title: "Dark mode" }, noun: "merge request" }, "x"), reviewPrompt({ number: 7, title: "Dark mode" }, "merge request", "x"));
  assert.equal(askContent({ kind: "branch", base: "develop" }), branchReviewPrompt("develop"));
});

test("the button and the sentence say where the review lands; the drafts are the checkout's", () => {
  assert.equal(askLabel({ kind: "pr", pr: { number: 1, title: "t" } }, "general-agent"), "Review with general-agent");
  assert.equal(askLabel({ kind: "branch", base: "main" }, "reviewer"), "Review against main with reviewer");
  assert.match(landsWords({ kind: "pr", pr: { number: 1, title: "t" } }), /posts its review on the pull request under its name\. You follow it here\.$/);
  assert.match(landsWords({ kind: "branch", base: "main" }), /against main .* replies in this checkout's conversation\. It changes nothing\. You follow it here\.$/);
  assert.doesNotMatch(landsWords({ kind: "pr", pr: { number: 1, title: "t" } }), /Agent tab/, "the step follows the run itself; no other panel is named");
  assert.deepEqual(draftKeys("workstream:abc"), {
    agent: "workstream:abc:review-agent",
    message: "workstream:abc:review-message",
    run: "workstream:abc:review-run",
    fixAgent: "workstream:abc:fix-agent",
    checkAgent: "workstream:abc:check-agent",
  });
  assert.equal(DEFAULT_AGENT, "general-agent");
});

test("the fix doors: a row's menu picks the agent there, leading with the last one used; the header counts the open ones", () => {
  assert.equal(FIX_MENU_LABEL, "Fix with");
  assert.match(fixLabel("reviewer"), /reviewer first, or another/);
  assert.match(fixLabel("reviewer"), /its commits land on this branch/);
  assert.equal(fixAllLabel(3), "Fix all 3 open");
  assert.equal(fixAllLabel(1), "Fix the open one");
});

test("the person's buttons are Approve, Submit review and Request changes in that order, only the ones the host takes, the first one primary", () => {
  assert.deepEqual(reviewButtons([...EVENTS]), [
    { event: "approve", label: "Approve", primary: true, danger: false },
    { event: "comment", label: "Submit review", primary: false, danger: false },
    { event: "request_changes", label: "Request changes", primary: false, danger: true },
  ]);
  assert.deepEqual(
    reviewButtons(["comment"]).map((b) => [b.label, b.primary]),
    [["Submit review", true]],
    "an own pull request: one plain button, and it is the primary one",
  );
  assert.deepEqual(
    reviewButtons(["approve", "comment"]).map((b) => b.event),
    ["approve", "comment"],
    "GitLab has no request-changes review",
  );
  assert.deepEqual(reviewButtons([]), []);
});

test("a comment or a change request without words is off with its reason; an approval never is", () => {
  assert.match(wordsReason("comment", ""), /A review needs words/);
  assert.match(wordsReason("request_changes", "  "), /A change request needs words/);
  assert.equal(wordsReason("approve", ""), null);
  assert.equal(wordsReason("comment", "one thing"), null);
});
