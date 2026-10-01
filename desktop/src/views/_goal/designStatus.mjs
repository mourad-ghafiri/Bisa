/**
 * What the goal page says while the Workflow Agent owes it a workflow.
 *
 * The node reports the agent's standing (`guidance.design`: the last guidance
 * fact, and whether a wake is live behind it); the bus streams its transitions
 * (`guided`) and its session's activity (`session`). This model turns those
 * into one card — a headline, a hint, a tone, the elapsed time while it
 * works, the agent's latest action, and which moves are offered — so the
 * screen never reads as "your move to pick a workflow" while somebody is
 * designing one, and never as "designing" when nobody is.
 *
 * Plain `.mjs` with a `.d.mts` beside it: the sentences and the offered moves
 * are the facts a person acts on, and `node --test` reaches them here.
 */

import { sessionLine } from "../../activityModel.mjs";
import { durationPrecise } from "../../i18n/format.mjs";
import { designs, modeOf } from "./goalMode.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** The kinds in which the Workflow Agent is at work: nobody picks a workflow over its head. */
export const DESIGNING_KINDS = ["scheduled", "working", "asking"];

/** Each kind as the card's chip says it. */
const KIND_WORD = Object.freeze({
  scheduled: tr("goal-design-status-kind-scheduled"),
  working: tr("goal-design-status-kind-working"),
  asking: tr("goal-design-status-kind-asking"),
  proposed: tr("goal-design-status-kind-proposed"),
  stalled: tr("goal-design-status-kind-stalled"),
  failed: tr("goal-design-status-kind-failed"),
  off: tr("goal-design-status-kind-off"),
  manual: tr("goal-design-status-kind-manual"),
});

/** The word of a kind, for the card's chip. */
export function kindWord(kind) {
  return KIND_WORD[kind] ?? String(kind ?? ""); // for the machine
}

/** Whether the card's clock ticks: the agent is at work, or about to be. */
export function ticks(kind) {
  return DESIGNING_KINDS.includes(kind);
}

/**
 * True while the Workflow Agent is designing (or repairing) this goal's
 * workflow — the moment a person may not choose one by hand. On a manual
 * goal, and after it stops (stalled, failed, off), they may.
 */
export function designInProgress(guidance, goal) {
  return ticks(designView(guidance, goal).kind);
}

function nowSecs() {
  return Math.floor(Date.now() / 1000);
}

/** How long the agent has been at it, in the platform's one way of saying a span (`i18n/format`); never negative. */
function elapsedLabel(sinceSecs, nowSecs) {
  return durationPrecise(nowSecs - sinceSecs);
}

/**
 * The card for a goal, from the node's view of it. `now` is injectable for
 * the elapsed label; `paused` says the engine is paused (a scheduled wake
 * sits until it resumes); `activity` is the agent's latest action, kept by
 * the caller from `session` frames.
 */
export function designView(guidance, goal, { now = nowSecs(), paused = false, activity = null } = {}) {
  const base = { elapsed: null, offerPick: false, offerRetry: false, offerAsk: !goal?.closed, activity: null, primary: null };
  const mode = modeOf(goal);
  if (!designs(mode)) {
    return {
      ...base,
      kind: "manual",
      headline: tr("goal-design-status-design-how-runs"),
      hint: tr("goal-design-status-goal-yours-design-draw-workflow-workflow"),
      tone: "quiet",
      primary: "design",
      offerPick: true,
    };
  }
  // What the agent's work leads to: an auto goal starts on its own, a
  // guided one is adopted by the person.
  const then = mode === "auto" ? tr("goal-design-status-then-starts-asked-only-where-step") : tr("goal-design-status-then-proposes-ll-adopt-workflow-tab");
  const ahead = tr("goal-design-status-reads-goal-decides-itself-may-ask", { flag: (mode === "auto") ? "yes" : "no", then });
  const design = guidance?.design ?? null;
  if (!design) {
    if (guidance && guidance.design_enabled === false) {
      return off(base);
    }
    return {
      ...base,
      kind: "scheduled",
      headline: paused ? tr("goal-design-status-workflow-agent-queued-engine-paused") : tr("goal-design-status-workflow-agent-about-design-goal-s"),
      hint: ahead,
      tone: "accent",
    };
  }
  const elapsed = elapsedLabel(design.since, now);
  switch (design.status) {
    case "scheduled":
      return {
        ...base,
        kind: "scheduled",
        headline: paused ? tr("goal-design-status-workflow-agent-queued-engine-paused") : tr("goal-design-status-workflow-agent-about-design-goal-s"),
        hint: ahead,
        tone: "accent",
        elapsed,
      };
    case "working":
      return {
        ...base,
        kind: "working",
        headline: design.phase === "repair" ? tr("goal-design-status-workflow-agent-repairing-goal-s-workflow") : tr("goal-design-status-workflow-agent-designing-goal-s-workflow"),
        hint: ahead,
        tone: "accent",
        elapsed,
        activity,
      };
    case "asking":
      return {
        ...base,
        kind: "asking",
        headline: tr("goal-design-status-workflow-agent-asked-question"),
        hint: design.detail ? tr("goal-design-status-answer-move-above-design-continues-from", { detail: design.detail }) : tr("goal-design-status-answer-move-above-design-continues-from-2"),
        tone: "warn",
        elapsed,
      };
    case "proposed":
      return {
        ...base,
        kind: "proposed",
        headline: tr("goal-design-status-workflow-proposed-review-adopt"),
        hint: design.detail ? tr("goal-design-status-review-every-step-then-adopt-start", { detail: design.detail }) : tr("goal-design-status-review-every-step-then-adopt-start-2"),
        tone: "ok",
        primary: "review",
      };
    case "stalled":
      return {
        ...base,
        kind: "stalled",
        headline: tr("goal-design-status-workflow-agent-stopped-without-proposal"),
        hint: design.detail ? tr("goal-design-status-retry-design-pick-workflow-hand", { detail: capitalise(design.detail) }) : tr("goal-design-status-retry-design-pick-workflow-hand-2"),
        tone: "danger",
        primary: "retry",
        offerRetry: true,
        offerPick: true,
      };
    case "failed":
      return {
        ...base,
        kind: "failed",
        headline: tr("goal-design-status-workflow-agent-could-not-start"),
        hint: design.detail ? tr("goal-design-status-retry-once-s-fixed-pick-workflow", { detail: capitalise(design.detail) }) : tr("goal-design-status-retry-once-s-fixed-pick-workflow-2"),
        tone: "danger",
        primary: "retry",
        offerRetry: true,
        offerPick: true,
      };
    case "off":
      return off(base);
    default:
      return {
        ...base,
        kind: "working",
        headline: tr("goal-design-status-workflow-agent", { status: String(design.status) }),
        hint: "",
        tone: "quiet",
        elapsed,
      };
  }
}

function off(base) {
  return {
    ...base,
    kind: "off",
    headline: tr("goal-design-status-designing-off-node"),
    hint: tr("goal-design-status-nobody-will-design-draw-or-pick"),
    tone: "quiet",
    primary: "pick",
    offerPick: true,
    offerAsk: false,
  };
}

function capitalise(text) {
  const t = String(text).trim().replace(/\.$/, "");
  return t.charAt(0).toUpperCase() + t.slice(1);
}

/**
 * A `guided` frame arrived for this goal: the design status it implies, so
 * the card moves before the page refetches. `at` is when it arrived.
 */
export function applyGuidedFrame(design, payload, at) {
  if (!payload || payload.type !== "guided") return design;
  const status = payload.status;
  return {
    phase: payload.phase,
    status,
    since: at,
    detail: payload.detail ?? null,
    session: payload.session ?? design?.session ?? null,
    live: status === "scheduled" || status === "working",
  };
}

/**
 * The agent's latest action, from a `session` frame: the same line the activity
 * would print for it, or the previous one when the frame is noise (a token,
 * a turn boundary). Any other payload leaves it as it was.
 */
export function activityFrom(payload, prev) {
  if (!payload || payload.type !== "session") return prev;
  const line = sessionLine(payload.event);
  return line ?? prev;
}
