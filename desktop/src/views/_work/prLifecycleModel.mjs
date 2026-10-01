/**
 * Where a branch is on its way to the base, as facts (ide/07, ide/08): the
 * seven steps — commit, push, open a pull request, checks, review, merge, clean
 * up — which are done, which are **waiting** (in flight, nobody's act: checks
 * running, the code host still computing), which are blocked and why, and
 * the **one** action that is legal right now with the **step it belongs to**
 * (`cta.step`). The step in hand — `current`, the one accent dot — is the
 * act's step whenever there is an act, so a merge button is drawn under
 * *Merge* and nowhere else, never under *Checks* because the checks happened
 * to be running. The Workstreams panel's lifecycle draws this as its spine
 * and hangs each step's surface under its row; before it existed the panel
 * showed every verb at once and let the node refuse four of them.
 *
 * **Facts first, the record second.** The record's state word lags a terminal:
 * a `git commit` in a shell leaves the record `open` while the branch is a
 * commit ahead of its base. So *commit* is done when there is anything beyond
 * the base, *push* when the upstream has nothing outstanding, and the record
 * decides only what git cannot know — whether a pull request was opened,
 * merged, or the workstream closed.
 *
 * **The review is optional.** While a pull request is open the Review step
 * is a todo that says so until anyone reviews, then done with who and what
 * they said (`reviewStepModel.reviewFacts`); it never gates the merge and
 * never claims the panel's one action — its surface (the agent request, the
 * person's buttons) is drawn under its row whatever its status. The Merge
 * step is **offered whenever the pull request is open**, and only what the
 * code host itself cannot merge **blocks** it: a draft, conflicts, failing
 * checks, unpushed commits, a mergeability the host is still computing (the
 * row *waiting* then, not blocked — nothing is wrong, the host is not done).
 * Checks still running, comments still open and a standing request for
 * changes are **cautions** the merge names — on the row and in the
 * confirmation, which then reads *Merge anyway* — while the merge stays legal.
 *
 * **An absent capability is not a gate.** A code host that reports no check runs
 * says so on its *Checks* step, and one that exposes no resolvable comments
 * cannot caution on "0 comments open". Only what the code host can say is asked of it.
 */

import { checksSummary } from "./prFormModel.mjs";
import { stepNote } from "./reviewStepModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The steps, in the order the work takes them. */
export const STEPS = Object.freeze(["commit", "push", "open_pr", "checks", "review", "merge", "cleanup"]);

const LABEL = Object.freeze({
  commit: t("work-git-words-commit"),
  push: t("work-git-words-push"),
  open_pr: t("work-pr-lifecycle-open-pull-request"),
  checks: t("work-checks-step-checks"),
  review: t("work-conflict-view-review"),
  merge: t("work-merge-control-merge"),
  cleanup: t("work-after-merge-dialog-clean-up"),
});

/**
 * A pull request's title before anyone typed one: the branch tip's commit
 * subject, else the branch — the two things a branch always has.
 * @param {string | null | undefined} subject
 * @param {string | null | undefined} branch
 */
export function prTitleFrom(subject, branch) {
  const line = String(subject ?? "").split("\n")[0]?.trim() ?? "";
  return line || String(branch ?? "");
}

/**
 * @typedef {{id: string, label: string, status: "done" | "current" | "waiting" | "todo" | "blocked", note: string | null, cautions: string[]}} Step
 * @typedef {{id: string, step: string, label: string, secondary?: boolean, blocked?: string, cautions?: string[], pushes?: boolean}} Cta — `pushes`: the verb pushes first, then opens
 *   the one legal act — what it does (`id`) and the step it is drawn under
 *   (`step`: *Push and open pull request* sits under Push, where the person is).
 */

/**
 * The lifecycle, from what the panel already knows.
 *
 * @param {object} facts
 * @param {boolean} facts.isGit            the workstream has a branch at all (a copy does not)
 * @param {string} facts.state             the record's state word
 * @param {number | null | undefined} facts.aheadOfBase   commits beyond the base
 * @param {string | null | undefined} facts.upstream      the branch's upstream, when pushed once
 * @param {number | null | undefined} facts.ahead         commits not on the upstream
 * @param {string | null | undefined} facts.base
 * @param {object | null | undefined} facts.pr            the code host's pull request, when read
 * @param {readonly object[] | null | undefined} facts.checks   check runs; null when not read
 * @param {import("./reviewStepModel.mjs").ReviewFacts} facts.review   `reviewFacts`' word on the reviews
 * @param {object | null | undefined} facts.caps          the code host's capabilities
 * @param {string} [facts.noun]                           the host's word — *pull request*, or GitLab's *merge request*
 * @returns {{steps: Step[], current: string | null, cta: Cta | null, note: string | null}}
 */
export function lifecycle({ isGit, state, aheadOfBase, upstream, ahead, base, pr, checks, review, caps, noun = t("work-publish-outcome-banner-pull-request") }) {
  if (!isGit) return { steps: [], current: null, cta: null, note: t("work-pr-lifecycle-copy-has-no-branch-nothing-here") };
  const closed = state === "closed";
  const merged = state === "merged" || pr?.state === "merged";
  const prOpened = merged || state === "pr_open";
  const beyond = (aheadOfBase ?? 0) > 0;
  const committed = beyond || prOpened || state === "committed" || state === "pushed";
  const outstanding = ahead ?? 0;
  const pushed = prOpened ? outstanding === 0 : Boolean(upstream) && committed && outstanding === 0;
  const prClosedOnHost = prOpened && !merged && pr?.state === "closed";

  const steps = [];
  let cta = null;
  const put = (id, status, note = null, cautions = []) =>
    steps.push({ id, label: id === "open_pr" ? t("work-pr-form-open-2", { noun }) : LABEL[id], status, note, cautions });

  // Commit.
  if (committed) put("commit", "done", beyond ? t("work-pr-lifecycle-beyond", { aheadOfBase, base: base ?? t("work-new-workstream-dialog-base") }) : null);
  else {
    put("commit", "current", t("work-pr-lifecycle-nothing-beyond-yet", { base: base ?? t("work-new-workstream-dialog-base") }));
    cta = { id: "commit", step: "commit", label: t("work-pr-lifecycle-commit-git-changes"), secondary: true };
  }

  // Push. *Push and open* sits under Push — that is where the person is.
  if (!committed) put("push", "todo");
  else if (pushed) put("push", "done", upstream ? t("work-pr-lifecycle-on-upstream", { upstream }) : null);
  else if (prOpened) {
    put("push", "current", t("work-pr-lifecycle-commit-commits-not-pushed", { outstanding }));
    cta = cta ?? { id: "push", step: "push", label: t("work-pr-lifecycle-push-git-changes"), secondary: true };
  } else {
    put("push", "current", upstream ? t("work-pr-lifecycle-commit-commits-not-pushed", { outstanding }) : t("work-pr-lifecycle-not-remote-yet"));
    cta = cta ?? { id: "open_pr", step: "push", label: t("work-pr-form-push-open", { noun }), pushes: true };
  }

  // Open a pull request.
  if (prOpened) put("open_pr", prClosedOnHost ? "blocked" : "done", prClosedOnHost ? t("work-pr-lifecycle-closed-code-host-without-merging") : pr ? `#${pr.number}` : null);
  else if (pushed) {
    put("open_pr", "current");
    cta = cta ?? { id: "open_pr", step: "open_pr", label: t("work-pr-form-open-2", { noun }) };
  } else put("open_pr", "todo");

  // Checks — always a step, so the spine never changes length under a person;
  // a code host that reports none says so on the step and is never a gate.
  // Runs still running are *waiting*: in flight, nobody's act.
  const summary = checks ? checksSummary([...checks]) : null;
  // Only a code host that reports check runs can fail them: a stale list from
  // elsewhere is not a gate on a code host that has no such thing.
  const checksFailing = caps?.check_runs === true && summary?.tone === "danger";
  // A dim summary of a non-empty list is runs still going; an empty list is *no checks*, and nothing is running — read from the list, never from the words.
  const checksRunning = caps?.check_runs === true && prOpened && !merged && summary !== null && summary.tone === "dim" && (checks?.length ?? 0) > 0;
  if (!caps?.check_runs) put("checks", prOpened ? "done" : "todo", t("work-pr-lifecycle-not-reported-code-host"));
  else {
    if (!prOpened || merged) put("checks", prOpened ? "done" : "todo", merged && summary ? summary.text : null);
    else if (!summary) put("checks", "waiting", t("work-pr-lifecycle-reading-code-host"));
    else if (checksFailing) put("checks", "blocked", summary.text);
    else if (checksRunning) put("checks", "waiting", summary.text);
    else put("checks", "done", summary.text);
  }

  // Review — optional, and never the panel's act: its surface is drawn under
  // its row whatever its status. It reports who reviewed and what they said.
  // Comments still open and a standing request for changes are the merge's
  // cautions; a code host that exposes no resolvable comments cannot have the first.
  const openComments = caps?.review_threads === true ? review.openComments : 0;
  const commentsWord = t("work-pr-lifecycle-comment-comments-open", { openComments });
  const reviewCautions = [];
  if (prOpened && !merged) {
    if (openComments > 0) reviewCautions.push(commentsWord);
    if (review.verdict === "changes_requested") reviewCautions.push(stepNote(review));
  }
  if (!prOpened) put("review", "todo");
  else if (merged) put("review", "done");
  else put("review", review.given ? "done" : "todo", stepNote(review), reviewCautions);

  // Merge — offered whenever the pull request is open; the code host's own
  // reasons are the only blocks. Checks still running lead its cautions.
  if (merged) put("merge", "done", pr?.head_sha ? t("work-pr-lifecycle-merged-as", { sha: String(pr.head_sha).slice(0, 7) }) : null);
  else if (!prOpened) put("merge", "todo");
  else {
    const cautions = [...(checksRunning ? [t("work-pr-lifecycle-checks-still-running")] : []), ...reviewCautions];
    const hold = mergeHold({ prClosedOnHost, pr, base, checksFailing, summary, outstanding });
    if (hold) {
      // Nothing is wrong while the host computes: the row waits, the act holds.
      put("merge", hold.waiting ? "waiting" : "blocked", hold.reason, cautions);
      cta = cta ?? { id: "merge", step: "merge", label: t("work-merge-control-merge"), blocked: hold.reason, cautions };
    } else {
      put("merge", "current", cautions.length > 0 ? cautions.join(" · ") : null, cautions);
      cta = cta ?? { id: "merge", step: "merge", label: t("work-merge-control-merge"), cautions };
    }
  }

  // Clean up — pull the base, delete the branch, go back.
  if (closed) put("cleanup", "done");
  else if (merged) {
    put("cleanup", "current");
    cta = cta ?? { id: "cleanup", step: "cleanup", label: t("work-pr-lifecycle-clean-up-branch") };
  } else put("cleanup", "todo");

  // The step in hand is the act's; with no act, the first thing in flight or in the way.
  const act = closed ? null : cta;
  const current = act?.step ?? steps.find((s) => s.status === "waiting" || s.status === "blocked")?.id ?? null;
  return { steps: oneInHand(steps, current), current, cta: act, note: closed ? t("work-pr-lifecycle-workstream-closed") : null };
}

/**
 * The line under the spine saying when the code host was last read — and,
 * while an agent works, that the lifecycle is following it: *code host read
 * 12 s ago*, *following general-agent · read 5 s ago*. `null` before the first
 * read with nobody followed; *following general-agent · reading the code
 * host…* before it with one.
 * @param {number | null | undefined} at unix seconds of the last read
 * @param {boolean} following an agent run is live
 * @param {string | null | undefined} agent the run's agent
 * @param {number} [now] unix seconds
 */
export function readWords(at, following, agent, now = Date.now() / 1000) {
  const who = agent || t("work-pr-lifecycle-agent");
  if (at == null) return following ? t("work-pr-lifecycle-following-reading-code-host", { who }) : null;
  const ago = agoWords(Math.max(0, Math.floor(now - at)));
  return following ? t("work-pr-lifecycle-following-read", { who, ago }) : t("work-pr-lifecycle-code-host-read", { ago });
}

/**
 * Why the merge is off, when it is: what the code host itself cannot merge —
 * closed there, a draft, conflicts, failing checks, commits not pushed —
 * **blocks**; a mergeability the host is still computing is **waited on**,
 * nothing being wrong. The two are told apart here by the fact, never by
 * comparing the sentence. `null` when the merge is legal.
 * @param {{prClosedOnHost: boolean, pr: object | null | undefined, base: string | null | undefined, checksFailing: boolean, summary: {text: string} | null, outstanding: number}} facts
 * @returns {{reason: string, waiting: boolean} | null}
 */
function mergeHold({ prClosedOnHost, pr, base, checksFailing, summary, outstanding }) {
  const block = (reason) => ({ reason, waiting: false });
  if (prClosedOnHost) return block(t("work-pr-lifecycle-closed-code-host"));
  if (pr?.is_draft) return block(t("work-pr-lifecycle-draft-mark-ready-code-host"));
  if (pr && pr.mergeable === false) return block(t("work-pr-lifecycle-conflicts", { base: pr.base ?? base ?? t("work-new-workstream-dialog-base") }));
  if (checksFailing) return block(summary?.text ?? t("work-pr-lifecycle-checks-failing"));
  if (pr && (pr.mergeable === null || pr.mergeable === undefined)) return { reason: t("work-pr-lifecycle-code-host-still-checking"), waiting: true };
  if (outstanding > 0) return block(t("work-pr-lifecycle-commits-not-pushed"));
  return null;
}

/** *just now* · *12 s ago* · *3 min ago* · *2 h ago*. @param {number} secs */
function agoWords(secs) {
  if (secs < 5) return t("work-pr-lifecycle-just-now");
  if (secs < 60) return t("work-pr-lifecycle-s-ago", { secs });
  if (secs < 3600) return t("work-pr-lifecycle-min-ago", { secs: Math.floor(secs / 60) });
  return t("work-pr-lifecycle-h-ago", { secs: Math.floor(secs / 3600) });
}

/**
 * One accent dot: the step in hand is `current`; any other step the facts
 * marked current — Push with commits outstanding while the act is the
 * merge's, say — reads as waiting on that fact instead.
 * @param {Step[]} steps @param {string | null} current
 */
function oneInHand(steps, current) {
  return steps.map((s) => (s.status === "current" && s.id !== current ? { ...s, status: "waiting" } : s));
}
