/**
 * The Review step's facts. Run with
 * `node --test desktop/src/views/_work/reviewStepModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  AGENT_REPLY_MARK,
  AGENT_REVIEW_MARK,
  EVENTS,
  activityWords,
  agentReply,
  allowedEvents,
  busyWords,
  clearsOnLanding,
  isFix,
  replierName,
  replierOf,
  replyWords,
  commentCount,
  commentsSummary,
  endWord,
  groupWords,
  needsWords,
  outcomeWords,
  ownPrNote,
  reviewFacts,
  reviewKey,
  reviewRun,
  reviewVerdict,
  reviewWords,
  reviewerOf,
  runWords,
  statusLine,
  stepNote,
  submittedWords,
  commentRow,
  unixOf,
  verdictWords,
} from "./reviewStepModel.mjs";

const GITHUB = { review_threads: true };
const agentReview = (agent, words, over = {}) => ({ author: "you", state: "commented", body: `${AGENT_REVIEW_MARK}${agent}\n\n${words}`, ...over });
const person = (author, state, body = "", over = {}) => ({ author, state, body, ...over });

test("the marks are the engine's constants, word for word", () => {
  const rust = readFileSync(new URL("../../../../crates/bisa-engine/src/codehost.rs", import.meta.url), "utf8");
  const m = rust.match(/pub const AGENT_REVIEW_MARK: &str = "([^"]+)";/);
  assert.ok(m, "the engine declares the review mark");
  assert.equal(AGENT_REVIEW_MARK, m[1]);
  const r = rust.match(/pub const AGENT_REPLY_MARK: &str = "([^"]+)";/);
  assert.ok(r, "the engine declares the reply mark");
  assert.equal(AGENT_REPLY_MARK, r[1]);
  assert.notEqual(AGENT_REPLY_MARK, AGENT_REVIEW_MARK, "a reply is not a review: its own words");
});

test("a reply's author is the agent the engine signed it with, else the person; its words lose the signature line", () => {
  const signed = { author: "you", body: `${AGENT_REPLY_MARK}fixer\n\nRenamed in abc123.` };
  assert.deepEqual(replierOf(signed), { kind: "agent", agent: "fixer" });
  assert.equal(replyWords(signed), "Renamed in abc123.");
  assert.equal(replierName(signed), "fixer");
  assert.deepEqual(replierOf({ author: "alice", body: "This rounds twice." }), { kind: "person", login: "alice" });
  assert.equal(replyWords({ author: "alice", body: " This rounds twice. " }), "This rounds twice.");
  assert.equal(replierName({ author: null, body: "x" }), "someone");
  assert.deepEqual(replierOf({ author: "you", body: `${AGENT_REVIEW_MARK}fixer\n\nnot a reply` }), { kind: "person", login: "you" }, "the review mark does not sign a reply");
  assert.deepEqual(replierOf({ author: "you", body: `${AGENT_REPLY_MARK}\n\nno id` }), { kind: "person", login: "you" }, "a mark with no id is not a signature");
});

test("a review's reviewer is the agent the engine signed it with, else the person the code host names", () => {
  assert.deepEqual(reviewerOf(agentReview("general-agent", "Looks good.")), { kind: "agent", agent: "general-agent" });
  assert.deepEqual(reviewerOf(person("alice", "approved", "ship it")), { kind: "person", login: "alice" });
  assert.deepEqual(reviewerOf({ author: null, body: "" }), { kind: "person", login: null });
  assert.deepEqual(reviewerOf({ author: "you", body: `${AGENT_REVIEW_MARK}\n\nno id` }), { kind: "person", login: "you" }, "a mark with no id is not a signature");
  assert.deepEqual(reviewerOf({ author: "you", body: `Reviewed by Bisa agent x said nothing` }), { kind: "agent", agent: "x said nothing" });
  assert.equal(reviewWords(agentReview("general-agent", "Looks good.\nOne nit.")), "Looks good.\nOne nit.");
  assert.equal(reviewWords({ author: "you", body: `${AGENT_REVIEW_MARK}general-agent` }), "", "a wordless approval is the mark alone");
  assert.equal(reviewWords(person("alice", "approved", " ship it ")), "ship it");
});

test("the verdict is each reviewer's latest review, changes requested outranking, dismissed clearing, pending ignored — an agent one reviewer whatever account posted it", () => {
  assert.equal(reviewVerdict([]), "none");
  assert.equal(reviewVerdict(null), "none");
  assert.equal(reviewVerdict([person("a", "approved")]), "approved");
  assert.equal(reviewVerdict([person("a", "approved"), person("b", "changes_requested")]), "changes_requested", "anyone's request outranks anyone's approval");
  assert.equal(reviewVerdict([person("a", "changes_requested"), person("a", "approved")]), "approved", "in array order, the later review is the author's word");
  assert.equal(
    reviewVerdict([person("a", "approved", "", { submitted_at: "2026-01-02T00:00:00Z" }), person("a", "changes_requested", "", { submitted_at: "2026-01-01T00:00:00Z" })]),
    "approved",
    "by time when the code host says when",
  );
  assert.equal(reviewVerdict([person("a", "changes_requested"), person("a", "dismissed")]), "none", "a dismissed review clears its author");
  assert.equal(reviewVerdict([person("a", "pending")]), "none");
  assert.equal(reviewVerdict([person("a", "commented")]), "commented");
  assert.equal(reviewVerdict([person(null, "approved"), person(null, "changes_requested")]), "changes_requested", "unnamed authors are one bucket, latest wins");
  assert.equal(
    reviewVerdict([person("you", "approved", "", { submitted_at: "2026-01-01T00:00:00Z" }), agentReview("ga", "Changes needed", { state: "changes_requested", submitted_at: "2026-01-02T00:00:00Z" })]),
    "changes_requested",
    "the agent's review is its own, not a later word from the same account",
  );
});

test("the review is optional: `given` says whether any stands, and the words say who and what — never that the merge waits", () => {
  const facts = (reviews, over = {}) => reviewFacts({ reviews, viewer: "you", comments: [], caps: GITHUB, ...over });
  const none = facts([]);
  assert.equal(none.given, false);
  assert.equal(none.agent, null);
  assert.equal(none.yours, null);
  assert.deepEqual(none.others, []);
  assert.equal(stepNote(none), "optional — an agent's, yours, or none");
  assert.equal(statusLine(none), "Optional: ask an agent to review, add your own, or merge as it is.");

  const agentOnly = facts([agentReview("general-agent", "Looks good.")]);
  assert.equal(agentOnly.given, true, "an agent's review alone is a review");
  assert.equal(agentOnly.agent.agent, "general-agent");
  assert.equal(agentOnly.yours, null, "the agent's comment under the connected account is not yours");
  assert.equal(stepNote(agentOnly), "reviewed by general-agent");
  assert.equal(statusLine(agentOnly), "Reviewed by general-agent. Add another, or merge.");

  const yoursOnly = facts([person("you", "commented", "fine by me")]);
  assert.equal(yoursOnly.given, true, "your review alone is a review");
  assert.equal(yoursOnly.yours.body, "fine by me");
  assert.equal(stepNote(yoursOnly), "reviewed by you");

  const both = facts([agentReview("general-agent", "Looks good."), person("you", "commented", "Agreed.")]);
  assert.equal(both.given, true);
  assert.equal(stepNote(both), "reviewed by general-agent and you");
  assert.equal(statusLine(both), "Reviewed by general-agent and you. Add another, or merge.");
  assert.equal(stepNote(facts([agentReview("a", "x"), person("you", "commented", "y"), person("bob", "commented", "z")])), "reviewed by a, you and @bob", "every reviewer, in order");

  const approved = facts([person("alice", "approved", "ship it")]);
  assert.equal(approved.given, true);
  assert.deepEqual(approved.others.map((r) => r.author), ["alice"]);
  assert.equal(stepNote(approved), "approved by @alice");
  assert.equal(statusLine(approved), "Approved by @alice.");

  const requested = facts([agentReview("general-agent", "Looks good."), person("you", "commented", "ok"), person("carol", "changes_requested", "no")]);
  assert.equal(requested.verdict, "changes_requested", "somebody's latest request for changes is the verdict whoever else reviewed");
  assert.equal(stepNote(requested), "changes requested by @carol");
  assert.equal(statusLine(requested), "Changes requested by @carol — address them, or merge anyway.");

  const rereviewed = facts([person("carol", "changes_requested", "no"), agentReview("general-agent", "Fixed."), person("you", "commented", "ok"), person("carol", "approved", "now yes")]);
  assert.equal(rereviewed.verdict, "approved", "the reviewer's later approval is their word");
  assert.equal(stepNote(rereviewed), "approved by @carol");

  const twoAgents = facts([agentReview("a", "first"), agentReview("b", "second")]);
  assert.equal(twoAgents.agent.agent, "b", "the latest agent review is the one shown");

  const noViewer = facts([person("you", "commented", "mine")], { viewer: null });
  assert.equal(noViewer.yours, null, "with the connection unread nothing is yours yet");
  assert.equal(noViewer.others.length, 1);

  assert.equal(facts([], { comments: [{ is_resolved: false }, { is_resolved: true }] }).openComments, 1);
  assert.equal(facts([], { comments: [{ is_resolved: false }], caps: { review_threads: false } }).openComments, 0, "a code host that exposes no resolvable comments has none open");
  assert.equal(verdictWords(person("a", "changes_requested")), "Changes requested");
});

test("the author of a pull request may only comment on it; anyone else may give a verdict the host takes; an unknown viewer is offered everything", () => {
  assert.deepEqual(allowedEvents({ author: "you" }, "you"), ["comment"]);
  assert.deepEqual(allowedEvents({ author: "alice" }, "you"), [...EVENTS]);
  assert.deepEqual(allowedEvents({ author: "you" }, null), [...EVENTS], "the connection unchecked: the code host has the last word");
  assert.deepEqual(allowedEvents({ author: null }, "you"), [...EVENTS]);
  assert.deepEqual(allowedEvents(null, "you"), [...EVENTS]);
  assert.deepEqual(allowedEvents({ author: "alice" }, "you", { review_events: ["approve", "comment"] }), ["approve", "comment"], "GitLab has no request-changes review");
  assert.deepEqual(allowedEvents({ author: "you" }, "you", { review_events: ["approve", "comment"] }), ["comment"]);
  assert.deepEqual(allowedEvents({ author: "alice" }, "you", { review_events: [] }), [...EVENTS], "a capabilities answer that says nothing offers all three");
  const note = ownPrNote({ author: "you" }, "you");
  assert.match(note, /^You opened this pull request as @you/);
  assert.equal(ownPrNote({ author: "alice" }, "you"), null);
  assert.equal(ownPrNote({ author: "you" }, null), null);
});

test("a comment or a change request needs words; an approval may stand alone; the toasts", () => {
  assert.equal(needsWords("comment", ""), true);
  assert.equal(needsWords("request_changes", "  "), true);
  assert.equal(needsWords("approve", ""), false);
  assert.equal(needsWords("comment", "one thing"), false);
  assert.equal(submittedWords("approve"), "Approved.");
  assert.equal(submittedWords("request_changes"), "Changes requested.");
  assert.equal(submittedWords("comment"), "Comment posted.");
  assert.equal(runWords({ kind: "branch", agent: "reviewer", at: 0 }), "reviewer is reviewing the branch…");
});

test("a comment folds to where, who, the first sentence and the reply count; a file group and the Comments header to their counts", () => {
  const row = commentRow({ path: "src/a.rs", line: 12, comments: [{ author: "alice", body: "This rounds twice. See `total()`." }, { author: "bob", body: "Agreed." }] });
  assert.deepEqual(row, { where: "line 12", author: "alice", lead: "This rounds twice.…", replies: 1 });
  assert.equal(commentRow({ path: "src/a.rs", line: 1, comments: [{ author: "you", body: `${AGENT_REPLY_MARK}reviewer\n\nRename this.` }] }).author, "reviewer", "an agent's comment is the agent's");
  assert.equal(commentRow({ path: "src/a.rs", line: 1, comments: [{ author: "you", body: `${AGENT_REPLY_MARK}reviewer\n\nRename this.` }] }).lead, "Rename this.", "without the signature line");
  assert.equal(commentRow({ path: null, line: null, comments: [] }).where, "general");
  assert.equal(commentRow({ path: "x", line: null, comments: [] }).where, "file");
  assert.equal(commentRow({ path: "x", line: 1, comments: [] }).author, "someone");
  assert.deepEqual(groupWords({ path: "src/a.rs", comments: [{ is_resolved: false }, { is_resolved: false }] }), { path: "src/a.rs", count: "2 comments · 2 open" });
  assert.deepEqual(groupWords({ path: "", comments: [{ is_resolved: true }] }), { path: "General", count: "1 comment · all resolved" });
  assert.equal(commentsSummary([]), "none");
  assert.equal(commentsSummary([{ is_resolved: true }, { is_resolved: true }]), "all resolved");
  assert.equal(commentsSummary([{ is_resolved: false }, { is_resolved: false }]), "2 open");
  assert.equal(commentsSummary([{ is_resolved: false }, { is_resolved: true }, { is_resolved: true }]), "1 open · 2 resolved");
  assert.deepEqual([commentCount(0), commentCount(1), commentCount(3)], ["the comments", "1 comment", "3 comments"]);
});

test("one agent review at a time: the run is its session, live or ended, starting for a while, done when it ends or the review lands — and how", () => {
  const run = { kind: "review", agent: "general-agent", at: 1_000 };
  const live = (over = {}) => ({ id: "s1", agent: "general-agent", workstream: "w1", state: "running", started: 1_001, children: [], ...over });
  assert.equal(reviewRun([], "w1", null, null), null, "no run, nothing");
  const starting = reviewRun([], "w1", run, reviewFacts({ reviews: [], viewer: "you", comments: [], caps: GITHUB }), false, 1_010);
  assert.deepEqual([starting.starting, starting.live, starting.done, starting.how, starting.session], [true, false, false, null, null], "the session has not appeared yet");
  const running = reviewRun([live()], "w1", run, null, false, 1_100);
  assert.deepEqual([running.starting, running.live, running.done, running.how], [false, true, false, null]);
  assert.deepEqual(running.session, { id: "s1", state: "running", started: 1_001, since: 1_001, subagents: 0 }, "a session that never said when its state began is since it started");
  assert.equal(reviewRun([live({ children: [{ id: "a" }, { id: "b" }] })], "w1", run, null, false, 1_100).session.subagents, 2, "the sub-agents are counted");
  assert.equal(reviewRun([live({ workstream: "w2" })], "w1", run, null, false, 1_100).session, null, "another workstream's session is not this run");
  assert.equal(reviewRun([live({ agent: "other" })], "w1", run, null, false, 1_100).session, null, "another agent's session is not this run");
  assert.equal(reviewRun([live({ started: 900 })], "w1", run, null, false, 1_100).session, null, "a session from before the ask is not this run");
  assert.equal(reviewRun([live({ started: 1_001, state: "done" }), live({ id: "s2", started: 1_050 })], "w1", run, null, false, 1_100).session.id, "s2", "the latest session is the run's");
  const ended = reviewRun([live({ state: "done" })], "w1", run, null, false, 1_100);
  assert.deepEqual([ended.done, ended.live, ended.how, ended.reason], [true, false, "done", null], "the session ended: the run is done, its session still known");
  assert.deepEqual([reviewRun([live({ state: "aborted" })], "w1", run, null, false, 1_100).how, reviewRun([live({ state: { state: "failed", reason: "no key" } })], "w1", run, null, false, 1_100).how], ["aborted", "failed"]);
  assert.equal(reviewRun([live({ state: { state: "failed", reason: "no key" } })], "w1", run, null, false, 1_100).reason, "no key", "a failure carries its reason");
  assert.equal(reviewRun([], "w1", run, null, false, 1_100).how, "gone", "no session after the grace: gone");
  const recorded = { ...run, ended: { at: 1_200, how: "stopped" } };
  assert.deepEqual([reviewRun([live()], "w1", recorded, null, false, 1_100).how, reviewRun([], "w1", recorded, null, false, 1_010).starting], ["stopped", false], "what the step recorded wins over the roster, and a recorded run never starts again");
  const landed = reviewFacts({ reviews: [agentReview("general-agent", "Looks good.", { submitted_at: "1970-01-01T00:20:00Z" })], viewer: "you", comments: [], caps: GITHUB });
  assert.equal(reviewRun([live()], "w1", run, landed, false, 1_100).how, "landed", "the review landed after the ask, whatever the session does next");
  assert.equal(clearsOnLanding(run, "landed"), true, "a landed review clears its run: the review row is the outcome");
  assert.equal(clearsOnLanding(run, "done"), false);
  const stale = reviewFacts({ reviews: [agentReview("general-agent", "Old.", { submitted_at: "1970-01-01T00:10:00Z" })], viewer: "you", comments: [], caps: GITHUB });
  assert.equal(reviewRun([live()], "w1", run, stale, false, 1_100).done, false, "an older review is not this run's");
  const fix = { kind: "fix", agent: "general-agent", at: 1_000 };
  assert.equal(reviewRun([live()], "w1", fix, landed, false, 1_100).done, false, "a fix run ends with its session, never with a review");
  assert.equal(clearsOnLanding(fix, "landed"), false);
  const branch = { kind: "branch", agent: "general-agent", at: 1_000 };
  assert.equal(reviewRun([live()], "w1", branch, landed, false, 1_100).done, false, "a branch review lands in the thread, not on a code host: only its session's end ends it");
  assert.equal(reviewRun([live({ state: "done" })], "w1", branch, null, false, 1_100).how, "done");
  // Idle is the moment after a session starts and the moment after its turn
  // ends: nothing runs, so Stop is not offered — and the run ends once the
  // reply is in the thread, or once it has sat idle the whole grace.
  const idleFresh = reviewRun([live({ state: "idle", since: 1_090 })], "w1", branch, null, false, 1_100);
  assert.deepEqual([idleFresh.starting, idleFresh.live, idleFresh.done, idleFresh.how], [false, false, false, null], "idle before the first turn: in progress, not stoppable");
  const idleReplied = reviewRun([live({ state: "idle", since: 1_090 })], "w1", branch, null, true, 1_100);
  assert.deepEqual([idleReplied.live, idleReplied.done, idleReplied.how], [false, true, "done"], "idle with the reply in the thread: the turn ended, the review is done");
  const idleLong = reviewRun([live({ state: "idle", since: 1_000 })], "w1", branch, null, false, 1_100);
  assert.deepEqual([idleLong.done, idleLong.how], [true, "done"], "idle the whole grace with no word: done, never stuck busy");
  assert.equal(reviewRun([live({ state: "parked" })], "w1", branch, null, false, 1_100).how, "done", "a parked session is kept by the engine and done for the run");
  const waiting = reviewRun([live({ state: { state: "waiting", on: { on: "auth", provider: "x" } } })], "w1", branch, null, false, 1_100);
  assert.ok(waiting.live && !waiting.done, "a session waiting on the person can still be stopped");
  assert.equal(runWords(run), "general-agent is reviewing…");
  assert.equal(runWords(fix), "general-agent is fixing the comments…");
  assert.equal(runWords({ ...fix, count: 2 }), "general-agent is fixing 2 comments…");
  assert.equal(runWords({ ...fix, count: 1, comment: "c9" }), "general-agent is fixing 1 comment…");
  // A check fix is a fix: it ends with its session, never with a review landing, and edits the checkout.
  const check = { kind: "check", agent: "fixer", at: 1_000, check: "ci / lint" };
  assert.equal(reviewRun([live({ agent: "fixer" })], "w1", check, landed, false, 1_100).done, false, "a check fix ends with its session, never with a review");
  assert.equal(reviewRun([live({ agent: "fixer", state: "done" })], "w1", check, null, false, 1_100).how, "done");
  assert.equal(clearsOnLanding(check, "landed"), false);
  assert.equal(runWords(check), "fixer is fixing check ci / lint…");
  assert.deepEqual([isFix(fix), isFix(check), isFix(run), isFix(branch)], [true, true, false, false]);
});

test("one agent at a time in a checkout: while a run is live every other door says who is busy and on what", () => {
  const state = (run, done = false) => ({ run, done });
  assert.equal(busyWords(null), null);
  assert.equal(busyWords(state({ kind: "fix", agent: "alpha", at: 0, count: 1, comment: "c1" }, true)), null, "an ended run holds nothing");
  assert.equal(busyWords(state({ kind: "fix", agent: "alpha", at: 0, count: 1, comment: "c1" }), "line 12"), "alpha is fixing line 12", "the fixed comment's place, when the caller knows it");
  assert.equal(busyWords(state({ kind: "fix", agent: "alpha", at: 0, count: 1, comment: "c1" })), "alpha is fixing 1 comment");
  assert.equal(busyWords(state({ kind: "fix", agent: "alpha", at: 0, count: 3 })), "alpha is fixing 3 comments");
  assert.equal(busyWords(state({ kind: "check", agent: "alpha", at: 0, check: "ci / lint" })), "alpha is fixing check ci / lint");
  assert.equal(busyWords(state({ kind: "review", agent: "alpha", at: 0 })), "alpha is reviewing");
  assert.equal(busyWords(state({ kind: "branch", agent: "alpha", at: 0 })), "alpha is reviewing the branch");
});

test("the run line says what the agent is doing this moment, then how it ended, in the step — never in another panel", () => {
  assert.equal(activityWords(null), "starting…");
  assert.equal(activityWords({ state: { state: "running", tool: "Bash", args: "cargo test" }, subagents: 0 }), "running Bash · cargo test");
  assert.equal(activityWords({ state: "thinking", subagents: 1 }), "thinking… · 1 sub-agent");
  assert.equal(activityWords({ state: "idle", subagents: 2 }), "getting ready…", "idle under a run is the moment before its turn");
  const review = { kind: "review", agent: "general-agent", at: 1_000 };
  const branch = { kind: "branch", agent: "reviewer", at: 1_000 };
  const fix = { kind: "fix", agent: "fixer", at: 1_000, count: 3, ahead: 2 };
  assert.equal(outcomeWords(review, "landed"), null, "a landed review's row is its outcome");
  assert.equal(outcomeWords(review, "done"), "general-agent ended without posting a review — its last words:");
  assert.equal(outcomeWords(branch, "done"), "reviewer reviewed the branch:");
  assert.equal(outcomeWords(fix, "done", 4), "fixer fixed 3 comments — 2 new commits on the branch, to keep or discard in Git › Changes.");
  assert.equal(outcomeWords(fix, "done", 3), "fixer fixed 3 comments — 1 new commit on the branch, to keep or discard in Git › Changes.");
  assert.equal(outcomeWords(fix, "done", 2), "fixer fixed 3 comments — no new commit on the branch, to keep or discard in Git › Changes.");
  assert.equal(outcomeWords({ ...fix, ahead: undefined }, "done", 4), "fixer fixed 3 comments — its commits are on the branch, to keep or discard in Git › Changes.", "no count before: no arithmetic");
  assert.equal(outcomeWords(fix, "done", null), "fixer fixed 3 comments — its commits are on the branch, to keep or discard in Git › Changes.", "no count after: no arithmetic");
  assert.equal(outcomeWords(review, "stopped"), "general-agent was stopped before reviewing.");
  assert.equal(outcomeWords(fix, "aborted"), "fixer was stopped mid-fix — see Git › Changes for what it changed.");
  const check = { kind: "check", agent: "fixer", at: 1_000, check: "ci / lint", ahead: 2 };
  assert.equal(outcomeWords(check, "done", 3), "fixer fixed check ci / lint — 1 new commit on the branch, to keep or discard in Git › Changes.");
  assert.equal(outcomeWords(check, "stopped"), "fixer was stopped mid-fix — see Git › Changes for what it changed.", "a check fix is a fix");
  assert.equal(outcomeWords(check, "failed", null, "no key"), "fixer failed — no key.");
  assert.equal(outcomeWords(review, "failed", null, "no key"), "general-agent failed — no key.");
  assert.equal(outcomeWords(review, "failed"), "general-agent failed.");
  assert.equal(outcomeWords(branch, "gone"), "reviewer's session ended before anything landed.");
  assert.deepEqual(["done", "landed", "stopped", "aborted", "gone", "failed"].map(endWord), ["done", "done", "aborted", "aborted", "aborted", "failed"], "the ended mark's word");
});

test("the agent's reply is its latest message after the ask, by the workspace's name for its author; a retracted one is not a reply", () => {
  const run = { kind: "branch", agent: "reviewer", at: 1_000 };
  const agentOf = (author) => ({ pk1: "reviewer", pk2: "general-agent" })[author] ?? null;
  const m = (id, author, created_at, over = {}) => ({ id, author, content: id, created_at, retracted: false, ...over });
  assert.equal(agentReply([], agentOf, run), null);
  assert.equal(agentReply([m("a", "pk1", 900)], agentOf, run), null, "before the ask: not this run's");
  assert.equal(agentReply([m("a", "pk2", 1_100), m("b", "you", 1_200)], agentOf, run), null, "another agent, a person: not the reply");
  assert.equal(agentReply([m("a", "pk1", 1_100), m("b", "pk1", 1_300), m("c", "pk1", 1_200)], agentOf, run).id, "b", "the latest");
  assert.equal(agentReply([m("a", "pk1", 1_100), m("b", "pk1", 1_300, { retracted: true })], agentOf, run).id, "a", "a retracted reply is not one");
  assert.equal(agentReply([m("a", "pk1", 1_100)], agentOf, null), null, "no run, no reply");
});

test("timestamps and keys", () => {
  assert.equal(unixOf("2026-01-01T00:00:00Z"), 1767225600);
  assert.equal(unixOf(null), null);
  assert.equal(unixOf("not a date"), null);
  assert.equal(reviewKey({ author: "alice", state: "approved", submitted_at: "t" }, 3), "alice:t");
  assert.equal(reviewKey({ author: null, state: "approved" }, 3), "?:3");
});
