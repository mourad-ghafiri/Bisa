/**
 * Asking an agent to review, and giving a review of your own, as facts
 * (ide/08): the prompt that asks an agent to review a **pull request** (its
 * review lands on the code host, a comment with the verdict in the words)
 * and the one that asks it to review a **branch against its base** (no pull
 * request — its review lands in the checkout's conversation, and it changes
 * nothing); the person's own words leading either; the buttons the person's
 * review offers — *Approve · Submit review · Request changes*, the ones the
 * code host takes; and the reason a button is off. "AgentReviewRequest.tsx"
 * and "ReviewStep.tsx" draw these; "node --test" checks them.
 */

import { needsWords } from "./reviewStepModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The person's words lead; the contract follows, a paragraph apart. */
function lead(message, contract) {
  const m = String(message ?? "").trim();
  return m ? `${m}\n\n${contract}` : contract;
}

/**
 * The prompt that asks an agent to review a pull request. The platform's own
 * credential opened it, so the code host takes a "comment" from the agent and
 * refuses an approval or a change request — the verdict goes in the words.
 * The engine signs the review's first line with the agent's id, so the Review
 * step can tell the agent's review from the person's; the agent is told not
 * to sign it itself.
 * @param {{number:number, title:string}} pr
 * @param {string} [noun] the host's word — *pull request*, or GitLab's *merge request*
 * @param {string} [message] what the person wants looked at, leading the prompt
 */
export function reviewPrompt(pr, noun = t("work-publish-outcome-banner-pull-request"), message = "") {
  // For the agent, never a person: English, as the model reads it.
  const contract = [
    `Please review ${noun} #${pr.number} — "${pr.title}".`,
    "Read the diff of this workstream's branch, judge the changes, and submit your review with the `pr_review_submit` tool using `event: comment`",
    "— the code host refuses an `approve` or `request_changes` from the account that opened the pull request, which is ours.",
    "Open the body with your verdict in one line (*Looks good* or *Changes needed*), then the findings; add inline `comments` on the exact changed lines for anything that must change.",
    "Be specific and cite files and lines. The platform signs the review with your agent id — do not sign it yourself.",
  ].join(" ");
  return lead(message, contract);
}

/**
 * The prompt that asks an agent to review a branch against its base, with no
 * pull request: the diff is "base...HEAD" in the checkout, the review is its
 * reply in this thread, and nothing changes — no edit, no commit, no push,
 * no pull request.
 * @param {string} base the branch the work goes back to
 * @param {string} [message] what the person wants looked at, leading the prompt
 */
export function branchReviewPrompt(base, message = "") {
  // For the agent, never a person: English, as the model reads it.
  const contract = [
    `Please review this workstream's branch against ${base}.`,
    `Run \`git diff ${base}...HEAD\` in this checkout, read the changes and judge them, then reply here with your review — there is no pull request to post on.`,
    "Open with your verdict in one line (*Looks good* or *Changes needed*), then the findings, each citing the file and the lines.",
    "Change nothing: no edits, no commits, no push, no pull request — the person decides what happens next.",
  ].join(" ");
  return lead(message, contract);
}

/**
 * @typedef {{kind: "pr", pr: {number: number, title: string}, noun?: string} | {kind: "branch", base: string}} ReviewTarget
 *   what the agent is asked to review: a pull request, or a branch against its base.
 */

/** The message to post for a target, the person's words first. @param {ReviewTarget} target @param {string} [message] */
export function askContent(target, message = "") {
  return target.kind === "pr" ? reviewPrompt(target.pr, target.noun ?? t("work-publish-outcome-banner-pull-request"), message) : branchReviewPrompt(target.base, message);
}

/** The words on the button that asks. @param {ReviewTarget} target @param {string} agent */
export function askLabel(target, agent) {
  return target.kind === "pr" ? t("work-agent-review-review", { agent }) : t("work-agent-review-review-against", { base: target.base, agent });
}

/** The sentence under the request, saying where the review lands — and that the step itself follows it. @param {ReviewTarget} target */
export function landsWords(target) {
  return target.kind === "pr"
    ? t("work-agent-review-agent-reads-diff-checkout-posts-review")
    : t("work-agent-review-agent-reads-diff-against-checkout-replies", { base: target.base });
}

/**
 * The words on the menu that hands one comment or one failed check to an
 * agent of the person's choice: *Fix with ▾* — the agent is picked on the
 * row itself, so the row never names one in advance.
 */
export const FIX_MENU_LABEL = t("work-agent-review-fix");

/**
 * The tooltip on a comment row's menu: the agent listed first is the last
 * one used, and picking any hands the comment over.
 * @param {string} remembered the agent the menu leads with
 */
export function fixLabel(remembered) {
  return t("work-agent-review-hand-comment-agent-first-another-commits", { remembered });
}

/**
 * The button on the Comments header that hands every open comment to one
 * agent, picked on the button: *Fix all 3 open* — *Fix the open one* when it
 * is one, so the words never read as a crowd.
 * @param {number} n how many are open
 */
export function fixAllLabel(n) {
  return n === 1 ? t("work-agent-review-fix-open-one") : t("work-agent-review-fix-all-open", { n });
}

/**
 * The drafts a request keeps in the checkout's session, by scope: the
 * reviewer, its words, the one run, and two remembered agents — the last
 * one a comment was handed to ("fixAgent", starting on the reviewer) and the
 * last one a failed check was ("checkAgent", starting on the fixer). Each
 * menu leads with its remembered agent; nothing is remembered per comment.
 * @param {string} scope
 */
export function draftKeys(scope) {
  return {
    agent: `${scope}:review-agent`,
    message: `${scope}:review-message`,
    run: `${scope}:review-run`,
    fixAgent: `${scope}:fix-agent`,
    checkAgent: `${scope}:check-agent`,
  };
}

/** The agent a request starts on when nothing was chosen here yet. */
export const DEFAULT_AGENT = "general-agent";

/** Every button the person's review can have, in order; the first offered is the primary one. */
const BUTTONS = Object.freeze([
  { event: "approve", label: t("work-agent-review-approve"), danger: false },
  { event: "comment", label: t("work-agent-review-submit-review"), danger: false },
  { event: "request_changes", label: t("work-agent-review-request-changes"), danger: true },
]);

/**
 * The buttons to draw for the verdicts the code host takes here
 * ("allowedEvents"), in the fixed order *Approve · Submit review · Request
 * changes*; the first is the primary one, so an own pull request — a comment
 * alone — has *Submit review* as its one plain button.
 * @param {readonly string[]} allowed
 * @returns {{event: string, label: string, primary: boolean, danger: boolean}[]}
 */
export function reviewButtons(allowed) {
  return BUTTONS.filter((b) => allowed.includes(b.event)).map((b, i) => ({ ...b, primary: i === 0 }));
}

/**
 * Why a button is off with the box empty — a comment or a change request
 * needs words; an approval may stand alone — or "null".
 * @param {string} event @param {string} body
 */
export function wordsReason(event, body) {
  if (!needsWords(event, body)) return null;
  return event === "request_changes" ? t("work-agent-review-change-request-needs-words-say-what") : t("work-agent-review-review-needs-words-say-what-noticed");
}
