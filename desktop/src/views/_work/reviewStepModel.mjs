/**
 * The Review step of the Workstreams panel's lifecycle, as facts (ide/08): who has
 * reviewed the pull request — an agent, you, somebody else on the code host —
 * where that leaves the step, and the words each state earns. `ReviewStep.tsx`
 * draws; `prLifecycleModel.lifecycle` holds the merge on it; `node --test`
 * checks.
 *
 * **The review is optional.** Nothing here gates the merge: a person may
 * merge with no review, with an agent's alone, or with their own alone. The
 * step *reports* — whether any review stands (`given`), whose, and the
 * verdict the code host would show (an approval, a request for changes, a
 * comment) — and the lifecycle turns a standing request for changes into a
 * caution the merge names, never a block.
 *
 * **How an agent's review is known.** One credential posts every review the
 * platform posts, so the code host's `author` is the connected account for the
 * agent's review and yours alike. The engine signs an agent's: the body's
 * first line is `AGENT_REVIEW_MARK` and the agent's id
 * (`crates/bisa-engine/src/codehost.rs`, held equal by the test).
 * `reviewerOf` reads it back; everything else here builds on that one fact.
 */

import { firstSentence } from "../../ui/foldModel.mjs";
import { isStoppable, stateOf } from "../../ui/sessionState.mjs";
import { headlineOf } from "../_workbench/workstreamPulseModel.mjs";
import { reviewEventsOf } from "./codeHostWords.mjs";
import { reviewStateLabel, unresolvedCount } from "./prReviewModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The first line of every review an agent submits, then its id — the engine's constant. */
export const AGENT_REVIEW_MARK = `${t("work-review-step-reviewed-by-agent")} `;

/** The first line of every reply an agent leaves on a comment, then its id — the engine's constant. */
export const AGENT_REPLY_MARK = `${t("work-review-step-reply-by-agent")} `;

/**
 * Who left a reply on a comment: the agent the engine signed it with, else
 * the person the code host names — the reply's twin of `reviewerOf`.
 * @param {{author?: string | null, body?: string | null}} reply
 * @returns {{kind: "agent", agent: string} | {kind: "person", login: string | null}}
 */
export function replierOf(reply) {
  const first = String(reply?.body ?? "").split("\n")[0]?.trim() ?? "";
  if (first.startsWith(AGENT_REPLY_MARK)) {
    const agent = first.slice(AGENT_REPLY_MARK.length).trim();
    if (agent) return { kind: "agent", agent };
  }
  return { kind: "person", login: reply?.author ?? null };
}

/**
 * A reply's words without the engine's signature line: an agent's from its
 * second paragraph on, a person's untouched.
 * @param {{author?: string | null, body?: string | null}} reply
 */
export function replyWords(reply) {
  const body = String(reply?.body ?? "");
  if (replierOf(reply).kind !== "agent") return body.trim();
  return body.split("\n").slice(1).join("\n").trim();
}

/** A reply's author as a row names it: the agent's id, the person's login, *someone*. @param {{author?: string | null, body?: string | null}} reply */
export function replierName(reply) {
  const who = replierOf(reply);
  return who.kind === "agent" ? who.agent : (who.login ?? "someone");
}

export const EVENTS = Object.freeze(["approve", "request_changes", "comment"]);

/** Which pull-request states outrank which, when one author reviewed twice. */
const VERDICT_RANK = Object.freeze({ changes_requested: 0, approved: 1, commented: 2 });

/** A review's states that count as a review given: not pending, not dismissed. */
const GIVEN = Object.freeze(["approved", "changes_requested", "commented"]);

/**
 * Who submitted a review: the agent the engine signed it with, else the
 * person the code host names.
 * @param {{author?: string | null, body?: string | null}} review
 * @returns {{kind: "agent", agent: string} | {kind: "person", login: string | null}}
 */
export function reviewerOf(review) {
  const first = String(review?.body ?? "").split("\n")[0]?.trim() ?? "";
  if (first.startsWith(AGENT_REVIEW_MARK)) {
    const agent = first.slice(AGENT_REVIEW_MARK.length).trim();
    if (agent) return { kind: "agent", agent };
  }
  return { kind: "person", login: review?.author ?? null };
}

/**
 * A review's words without the engine's signature line: an agent's body from
 * its second paragraph on, a person's untouched.
 * @param {{author?: string | null, body?: string | null}} review
 */
export function reviewWords(review) {
  const body = String(review?.body ?? "");
  if (reviewerOf(review).kind !== "agent") return body.trim();
  return body.split("\n").slice(1).join("\n").trim();
}

/**
 * The reviews in the order they were given: by `submitted_at` when the code
 * host says when, else the order they came in.
 * @template {{submitted_at?: string | null}} R
 * @param {readonly R[] | null | undefined} reviews
 * @returns {R[]}
 */
function ordered(reviews) {
  return (reviews ?? [])
    .map((r, i) => ({ r, i }))
    .sort((a, b) => {
      const ta = a.r.submitted_at ? Date.parse(a.r.submitted_at) : Number.NaN;
      const tb = b.r.submitted_at ? Date.parse(b.r.submitted_at) : Number.NaN;
      if (Number.isFinite(ta) && Number.isFinite(tb) && ta !== tb) return ta - tb;
      return a.i - b.i;
    })
    .map(({ r }) => r);
}

/**
 * The pull request's verdict from its reviews: each author's **latest** review
 * counts, a *dismissed* review clears that author, a *pending* one is not a
 * review yet, and *changes requested* by anyone outranks an approval by anyone
 * else — the way every code host's own badge reads it. An agent's reviews are
 * one author's, whatever account posted them.
 * @param {readonly {author?: string | null, body?: string | null, state: string, submitted_at?: string | null}[] | null | undefined} reviews
 * @returns {"approved" | "changes_requested" | "commented" | "none"}
 */
export function reviewVerdict(reviews) {
  const latest = new Map();
  for (const r of ordered(reviews)) {
    const who = reviewerKey(r);
    if (r.state === "pending") continue;
    if (r.state === "dismissed") latest.delete(who);
    else if (r.state in VERDICT_RANK) latest.set(who, r.state);
  }
  let best = null;
  for (const state of latest.values()) {
    if (best === null || VERDICT_RANK[state] < VERDICT_RANK[best]) best = state;
  }
  return best ?? "none";
}

/** One bucket per reviewer: the agent by id, a person by login. */
function reviewerKey(review) {
  const who = reviewerOf(review);
  return who.kind === "agent" ? `agent:${who.agent}` : `person:${who.login ?? ""}`;
}

/**
 * @typedef {{author?: string | null, body: string, state: string, submitted_at?: string | null}} Summary
 * @typedef {{
 *   verdict: "approved" | "changes_requested" | "commented" | "none",
 *   agent: {review: Summary, agent: string} | null,
 *   yours: Summary | null,
 *   others: Summary[],
 *   openComments: number,
 *   given: boolean,
 * }} ReviewFacts
 */

/**
 * Everything the step needs, from the code host's reviews and comments.
 *
 * `agent` is the latest review an agent gave; `yours` the latest the connected
 * account gave in person (`viewer` — null while the connection is unread, and
 * then nothing is yours yet); `others` the latest review of every other
 * account, in the order given. Only the comments a code host exposes as
 * resolvable (`review_threads`, the wire's word) count as open.
 *
 * @param {object} facts
 * @param {readonly Summary[] | null | undefined} facts.reviews
 * @param {string | null | undefined} facts.viewer   the connected account's login
 * @param {readonly {is_resolved: boolean}[] | null | undefined} facts.comments
 * @param {{review_threads?: boolean} | null | undefined} facts.caps
 * @returns {ReviewFacts}
 */
export function reviewFacts({ reviews, viewer, comments, caps }) {
  let agent = null;
  let yours = null;
  const others = new Map();
  for (const r of ordered(reviews)) {
    if (!GIVEN.includes(r.state)) continue;
    const who = reviewerOf(r);
    if (who.kind === "agent") agent = { review: r, agent: who.agent };
    else if (viewer && who.login === viewer) yours = r;
    else others.set(who.login ?? "", r);
  }
  const verdict = reviewVerdict(reviews);
  const openComments = caps?.review_threads === true ? unresolvedCount(comments ?? []) : 0;
  const given = agent !== null || yours !== null || others.size > 0;
  return { verdict, agent, yours, others: [...others.values()], openComments, given };
}

/** Who has reviewed, as words: the agent's id, *you*, each other account — in that order, joined by *and*. @param {ReviewFacts} facts */
function reviewerNames(facts) {
  const names = [];
  if (facts.agent) names.push(facts.agent.agent);
  if (facts.yours) names.push("you");
  for (const r of facts.others) names.push(r.author ? `@${r.author}` : "someone");
  if (names.length <= 1) return names[0] ?? "nobody";
  return t("work-review-step-words-2", { names: names.slice(0, -1).join(", "), names2: names[names.length - 1] });
}

/** The account whose latest review is the verdict, for the words. @param {ReviewFacts} facts */
function verdictBy(facts) {
  const candidates = [...facts.others, ...(facts.yours ? [facts.yours] : []), ...(facts.agent ? [facts.agent.review] : [])];
  const hit = ordered(candidates)
    .reverse()
    .find((r) => r.state === facts.verdict);
  if (!hit) return null;
  const who = reviewerOf(hit);
  return who.kind === "agent" ? who.agent : who.login ? `@${who.login}` : "someone";
}

/** The lifecycle row's note for the Review step: optional until someone reviews, then who and what they said. @param {ReviewFacts} facts */
export function stepNote(facts) {
  if (!facts.given) return t("work-review-step-optional-agent-s-yours-none");
  switch (facts.verdict) {
    case "approved":
      return t("work-review-step-approved", { facts: verdictBy(facts) ?? t("work-review-step-reviewer") });
    case "changes_requested":
      return t("work-review-step-changes-requested", { facts: verdictBy(facts) ?? t("work-review-step-reviewer") });
    default:
      return t("work-review-step-reviewed", { facts: reviewerNames(facts) });
  }
}

/** The sentence over the step's surface. @param {ReviewFacts} facts */
export function statusLine(facts) {
  if (!facts.given) return t("work-review-step-optional-ask-agent-review-add-own");
  switch (facts.verdict) {
    case "approved":
      return t("work-review-step-approved-2", { facts: verdictBy(facts) ?? t("work-review-step-reviewer") });
    case "changes_requested":
      return t("work-review-step-changes-requested-address-them-merge-anyway", { facts: verdictBy(facts) ?? t("work-review-step-reviewer") });
    default:
      return t("work-review-step-reviewed-add-another-merge", { facts: reviewerNames(facts) });
  }
}

/**
 * What a given review says, in a row's words: *Approved* · *Changes requested*
 * · *Commented*, with who when the reviewer is not the row's own subject.
 * @param {Summary} review
 */
export function verdictWords(review) {
  return reviewStateLabel(review.state);
}

/**
 * The verdicts the person may give: the ones the code host takes at all
 * (`capabilities().review_events` — GitLab has no *request changes*), and on
 * a pull request the connected account opened itself, a comment alone, since
 * every code host refuses its author's approval or change request. With no
 * viewer known (the connection unchecked) every verdict the host takes is
 * offered and the code host has the last word.
 * @param {{author?: string | null} | null | undefined} pr
 * @param {string | null | undefined} viewer the connected account's login
 * @param {{review_events?: readonly string[]} | null | undefined} [caps]
 */
export function allowedEvents(pr, viewer, caps = null) {
  const author = pr?.author ?? null;
  const takes = reviewEventsOf(caps);
  if (author && viewer && author === viewer) return takes.includes("comment") ? ["comment"] : [];
  return EVENTS.filter((e) => takes.includes(e));
}

/**
 * @typedef {"done" | "aborted" | "failed" | "stopped" | "landed" | "gone"} RunEnd
 *   how a run ended: its session finished, was aborted elsewhere, failed,
 *   was **stopped** from the step, its review **landed** on the code host
 *   (a pull request's review only), or no session ever showed (`gone`).
 * @typedef {{
 *   kind: "review" | "fix" | "branch" | "check",
 *   agent: string,
 *   at: number,
 *   conversation?: string,
 *   ahead?: number,
 *   count?: number,
 *   comment?: string,
 *   check?: string,
 *   ended?: {at: number, how: RunEnd, reason?: string},
 * }} ReviewRun
 *   what an agent was asked for — a pull request's review, comments fixed
 *   (`count` of them; `comment` the one's id when it was one), a branch
 *   reviewed against its base with no pull request, a failed check fixed
 *   (`check`, the run's name) — when (unix seconds), which agent, how many
 *   commits the branch had beyond its base at the ask (`ahead`, so a fix can
 *   say how many it added), and — once the step saw it end — how (`ended`,
 *   recorded so the outcome outlives the roster). Kept in the checkout's
 *   session so a tab switch keeps it. One run at a time in a checkout: two
 *   agents never edit the same tree at once.
 */

/**
 * The agent session a review run is, or was: the session in this workstream
 * that the run's agent started at or after the run was asked for — live or
 * ended, the latest when there are several. The run **ends** when the step
 * has recorded it (`run.ended`), when — for a pull request's review — an
 * agent review newer than the ask has landed (a branch review lands in the
 * thread, not on a code host, and a fix lands as commits, so only their
 * session's end ends them), when its session reached a terminal state or
 * parked, when its session sits **idle** with the reply in the thread
 * (`replied` — the turn ended and its words landed) or has sat idle for
 * `GRACE_SECS` with none (a turn that ended without a word), or when no
 * session showed within `GRACE_SECS` of the ask (a session takes a moment to
 * appear, so a run with no session yet is *starting*, not ended). A session
 * idle before its first turn — the moment after it started — is in progress
 * and not `live`: nothing runs yet, so nothing is offered to stop.
 * `live` is the one Stop rule (`isStoppable`): the session runs or waits on
 * the person. Answers `null` when there is no run, else the run with its
 * session, whether it is starting or live, and `how` it ended — `null`
 * while it has not.
 * @param {readonly {id: string, agent?: string | null, workstream?: string | null, state: string | {kind: string, reason?: string}, started: number, since: number, children?: readonly unknown[]}[]} sessions
 * @param {string} wid
 * @param {ReviewRun | null | undefined} run
 * @param {ReviewFacts | null | undefined} facts
 * @param {boolean} [replied] the agent's reply to this ask is in the thread
 * @param {number} [now] unix seconds
 * @returns {{run: ReviewRun, session: {id: string, state: string | {kind: string, reason?: string}, started: number, since: number, subagents: number} | null, starting: boolean, live: boolean, done: boolean, how: RunEnd | null, reason: string | null} | null}
 */
export function reviewRun(sessions, wid, run, facts, replied = false, now = Date.now() / 1000) {
  if (!run) return null;
  const landed =
    run.kind === "review" &&
    facts?.agent?.agent === run.agent &&
    (unixOf(facts.agent.review.submitted_at) ?? Number.NEGATIVE_INFINITY) >= run.at;
  const found = (sessions ?? [])
    .filter((s) => s.workstream === wid && (s.agent ?? null) === run.agent && s.started >= run.at - 1)
    .sort((a, b) => b.started - a.started)[0];
  const session = found ? { id: found.id, state: found.state, started: found.started, since: found.since ?? found.started, subagents: (found.children ?? []).length } : null;
  const live = session !== null && isStoppable(session.state);
  const word = session ? stateOf(session.state) : null;
  const settled = word === "idle" && (replied || now - session.since >= GRACE_SECS);
  const starting = session === null && !run.ended && now - run.at < GRACE_SECS;
  let how = run.ended?.how ?? null;
  let reason = run.ended?.reason ?? null;
  if (how === null) {
    if (landed) how = "landed";
    else if (session !== null && word !== "idle" && !live) {
      how = TERMINAL_ENDS[word] ?? "done";
      reason = typeof session.state === "object" && session.state?.reason ? String(session.state.reason) : null;
    } else if (settled) how = "done";
    else if (session === null && !starting) how = "gone";
  }
  return { run, session, starting, live: live && how === null, done: how !== null, how, reason };
}

/** A session's terminal word, as the run's end; parked is kept by the engine and done for the run. */
const TERMINAL_ENDS = Object.freeze({ done: "done", aborted: "aborted", failed: "failed", parked: "done" });

/** How long a run waits for its session to appear — and how long an idle session with no reply keeps it open — before it counts as ended. */
const GRACE_SECS = 45;

/**
 * What the agent is doing right now, for the run line: the session's
 * headline — *running Bash · src/cart.rs*, *thinking…*, *waiting on you —
 * permission: Bash* — and its sub-agents when it has any.
 * @param {{state: string | object, subagents: number} | null | undefined} session
 */
export function activityWords(session) {
  if (!session) return "starting…";
  // Idle before the first turn: the session is up and about to begin.
  if (stateOf(session.state) === "idle") return t("work-review-step-getting-ready");
  const words = headlineOf(session.state);
  const n = session.subagents ?? 0;
  return n > 0 ? t("work-review-step-sub-agent-agents", { words, n }) : words;
}

/**
 * The sentence a run ends on, by what was asked and how it ended; `null`
 * for a pull request's review that landed — the review row is its outcome.
 * A fix says how many commits it added when the branch's count is known
 * before and after (`run.ahead`, `aheadNow`).
 * @param {ReviewRun} run
 * @param {RunEnd} how
 * @param {number | null | undefined} [aheadNow] commits beyond the base now
 * @param {string | null | undefined} [reason] a failure's reason
 */
export function outcomeWords(run, how, aheadNow = null, reason = null) {
  const a = run.agent;
  const fixing = isFix(run);
  switch (how) {
    case "landed":
      return null;
    case "failed":
      return t("work-review-step-failed-failed", { a, reason, flag: (reason) ? "yes" : "no" });
    case "aborted":
    case "stopped":
      return fixing ? t("work-review-step-stopped-mid-fix-see-git-changes", { a }) : t("work-review-step-stopped-before-reviewing", { a });
    case "gone":
      return t("work-review-step-s-session-ended-before-anything-landed", { a });
    default:
      if (run.kind === "review") return t("work-review-step-ended-without-posting-review-last-words", { a });
      if (run.kind === "branch") return t("work-review-step-reviewed-branch", { agent: a });
      if (run.kind === "check") return t("work-review-step-fixed-keep-discard-git-changes", { a, run: checkName(run), aheadNow: commitsAdded(run, aheadNow) });
      return t("work-review-step-fixed-keep-discard-git-changes", { a, run: commentCount(run.count ?? 0), aheadNow: commitsAdded(run, aheadNow) });
  }
}

/** A run that edits the checkout — a comment fix or a check fix — as against one that only reads it. @param {ReviewRun} run */
export function isFix(run) {
  return run.kind === "fix" || run.kind === "check";
}

/** *check ci / lint*, or *the check* when the run forgot its name. @param {ReviewRun} run */
function checkName(run) {
  return run.check ? t("work-review-step-check-named", { check: run.check }) : t("work-review-step-check");
}

/**
 * Why nothing else can be asked right now: the one run this checkout has, in
 * words for the surface that wanted to ask — *alpha is fixing line 12*,
 * *alpha is fixing 3 comments*, *alpha is fixing check ci / lint*, *alpha is
 * reviewing* — or `null` when nothing is running. One agent at a time in a
 * checkout: two agents never edit the same tree at once, so every other door
 * says who is busy instead of opening.
 * @param {{run: ReviewRun, done: boolean} | null | undefined} state the run against the roster
 * @param {string} [where] the fixed comment's place (`commentRow(...).where`), when the caller knows it
 */
export function busyWords(state, where = null) {
  if (!state || state.done) return null;
  const { run } = state;
  const a = run.agent;
  switch (run.kind) {
    case "review":
      return t("work-review-step-reviewing", { a });
    case "branch":
      return t("work-review-step-reviewing-branch", { a });
    case "check":
      return t("work-review-step-fixing", { a, run: checkName(run) });
    default:
      if (run.comment && where) return t("work-review-step-fixing-2", { a, where });
      return t("work-review-step-fixing", { a, run: commentCount(run.count ?? 0) });
  }
}

/** *2 new commits on the branch*, or *its commits are on the branch* when the count is not known both before and after. */
function commitsAdded(run, aheadNow) {
  if (run.ahead == null || aheadNow == null) return t("work-review-step-commits-branch");
  const n = Math.max(0, aheadNow - run.ahead);
  if (n === 0) return t("work-review-step-no-new-commit-branch");
  return t("work-review-step-new-commit-commits-branch", { n });
}

/**
 * The agent's latest reply after the ask — the run's outcome in its own
 * words — from the checkout's conversation; `null` until it has said anything.
 * `agentOf` names a message's author as an agent id (the workspace's
 * pubkey map), so this stays pure.
 * @template {{author: string, content: string, created_at: number, retracted?: boolean}} M
 * @param {readonly M[] | null | undefined} messages
 * @param {(author: string) => string | null | undefined} agentOf
 * @param {ReviewRun | null | undefined} run
 * @returns {M | null}
 */
export function agentReply(messages, agentOf, run) {
  if (!run) return null;
  let best = null;
  for (const m of messages ?? []) {
    if (m.retracted || m.created_at < run.at) continue;
    if ((agentOf(m.author) ?? null) !== run.agent) continue;
    if (best === null || m.created_at >= best.created_at) best = m;
  }
  return best;
}

/**
 * The session word an ended run's mark wears: a stop is *aborted*, a
 * failure *failed*, a landing or a finish *done*, a session that never
 * showed *aborted* too — nothing came of it.
 * @param {RunEnd} how
 * @returns {"done" | "aborted" | "failed"}
 */
export function endWord(how) {
  switch (how) {
    case "failed":
      return "failed";
    case "aborted":
    case "stopped":
    case "gone":
      return "aborted";
    default:
      return "done";
  }
}

/** A pull request's review that landed has its row as the outcome: the run clears. @param {ReviewRun} run @param {RunEnd | null} how */
export function clearsOnLanding(run, how) {
  return run.kind === "review" && how === "landed";
}

/**
 * The run line's words while an agent works: what it was asked, in the
 * present; `activityWords` says what it is doing this moment.
 * @param {ReviewRun} run
 */
export function runWords(run) {
  switch (run.kind) {
    case "review":
      return t("work-review-step-reviewing-2", { agent: run.agent });
    case "branch":
      return t("work-review-step-reviewing-branch-2", { agent: run.agent });
    case "check":
      return t("work-review-step-fixing-3", { agent: run.agent, run: checkName(run) });
    default:
      return t("work-review-step-fixing-3", { agent: run.agent, run: commentCount(run.count ?? 0) });
  }
}

/** `1 comment` · `3 comments` · `the comments` when the number is unknown. */
export function commentCount(n) {
  if (!n) return t("work-review-step-comments-2");
  return t("work-review-step-comment-comments", { n });
}

/**
 * The sentence beside the verdict control on the author's own pull request;
 * `null` when the person is not its author (or the viewer is unknown).
 * @param {{author?: string | null} | null | undefined} pr
 * @param {string | null | undefined} viewer
 */
export function ownPrNote(pr, viewer) {
  if (allowedEvents(pr, viewer).length !== 1) return null;
  return t("work-review-step-opened-pull-request-so-code-host", { viewer });
}

/** A comment or a change request needs words; an approval may stand alone. */
export function needsWords(event, body) {
  return event !== "approve" && !String(body ?? "").trim();
}

/** The toast after a review went out. */
export function submittedWords(event) {
  switch (event) {
    case "approve":
      return t("work-review-step-approved-3");
    case "request_changes":
      return t("work-review-step-changes-requested-2");
    default:
      return t("work-review-step-comment-posted");
  }
}

/**
 * A comment folded to one row: where it is, who left it and what they said
 * first, how many replies follow.
 * @param {{path?: string | null, line?: number | null, comments: readonly {author?: string | null, body: string}[]}} comment
 */
export function commentRow(comment) {
  const first = comment.comments[0];
  return {
    where: comment.line != null ? t("work-review-step-where-line", { line: comment.line }) : comment.path ? t("work-review-step-where-file") : t("work-review-step-where-general"),
    author: first ? replierName(first) : t("work-review-step-model-someone"),
    lead: firstSentence(first ? replyWords(first) : ""),
    replies: Math.max(0, comment.comments.length - 1),
  };
}

/** The words on a file group's header. @param {{path: string, comments: readonly {is_resolved: boolean}[]}} group */
export function groupWords(group) {
  const open = unresolvedCount(group.comments);
  const total = group.comments.length;
  const count = open > 0 ? t("work-review-step-open", { open }) : t("work-review-step-all-resolved");
  return { path: group.path || t("work-review-step-general"), count: `${commentCount(total)} · ${count}` };
}

/** The Comments header's summary: `3 open · 2 resolved`, `all resolved`, or `none`. */
export function commentsSummary(comments) {
  const total = (comments ?? []).length;
  if (total === 0) return "none";
  const open = unresolvedCount(comments);
  if (open === 0) return t("work-review-step-all-resolved");
  return open === total ? t("work-review-step-open", { open }) : t("work-review-step-open-resolved", { open, open2: total - open });
}

/** ISO 8601 → unix seconds, or null. @param {string | null | undefined} iso */
export function unixOf(iso) {
  if (!iso) return null;
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? Math.floor(ms / 1000) : null;
}

/** A stable key for a review row. */
export function reviewKey(review, index) {
  return `${review.author ?? "?"}:${review.submitted_at ?? index}`;
}
