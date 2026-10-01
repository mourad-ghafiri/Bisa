/**
 * Settings › Decision Making, pure over the node's `GET /decisions/status` and the
 * `decisions.*` settings. The panel draws; this file decides which of a
 * provider's own fields show, what a decision point reads as, how the
 * points-off list is edited, what a *Try it* request and its answer read as,
 * and how a recorded judgement reads as one line.
 *
 * A judgement is never a "decision": kind 3400/3401's `decision` is a
 * person's signed approval. The words here say "judgement" throughout.
 */

import { agoWords } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { percent } from "../../i18n/format.mjs";

/** The `decisions.*` keys the panel edits, spelled once. */
export const KEYS = {
  enabled: "decisions.enabled",
  pointsOff: "decisions.points_off",
  provider: "decisions.provider",
  harness: { id: "decisions.harness.id", model: "decisions.harness.model", effort: "decisions.harness.effort" },
  agent: { id: "decisions.agent.id" },
  jev: { model: "decisions.jev.model" },
  rlcd: { endpoint: "decisions.rlcd.endpoint", model: "decisions.rlcd.model", auth: "decisions.rlcd.auth" },
  deadline: "decisions.deadline_secs",
  retries: "decisions.retries",
  confidenceAct: "decisions.confidence.act",
  confidenceSecurity: "decisions.confidence.security",
};

/** The scalar keys the registry-generated section shows, once the provider's own fields are drawn. */
export const SCALARS = [KEYS.deadline, KEYS.retries, KEYS.confidenceAct, KEYS.confidenceSecurity];

/**
 * The eleven decision points — the places the Decision-Making Agent may judge
 * instead of a place's own rule — in the order the node lists them
 * (`DecisionPoint::ALL`): a label for the switch row, and a one-sentence
 * description of what is being decided. The two model points lead: which
 * model, then how hard it works — each selected where it is used, by a
 * strategy or an effort of `auto`, and so never a switch of this panel's.
 */
export const POINTS = Object.freeze({
  "model.route": {
    label: t("settings-decisions-which-model-leads"),
    description: t("settings-decisions-under-auto-route-strategy-which-model"),
  },
  "model.effort": {
    label: t("settings-decisions-how-hard-model-works"),
    description: t("settings-decisions-under-effort-auto-which-level-model"),
  },
  "security.tool": {
    label: t("settings-decisions-tool-call-safe"),
    description: t("settings-decisions-when-classifier-s-provider-decision-making-agent"),
  },
  "security.message": {
    label: t("settings-decisions-person-s-message-safe"),
    description: t("settings-decisions-whether-message-from-person-another-node"),
  },
  "security.content": {
    label: t("settings-decisions-what-agent-reads-safe"),
    description: t("settings-decisions-whether-content-agent-reads-from-outside"),
  },
  "assign.pick": {
    label: t("settings-decisions-who-picks-up-work-item"),
    description: t("settings-decisions-which-agent-pool-takes-work-item"),
  },
  "dispatch.triage": {
    label: t("settings-decisions-who-answers-unaddressed-message"),
    description: t("settings-decisions-which-enabled-agent-answers-message-nobody"),
  },
  "goal.adopt": {
    label: t("settings-decisions-whether-designed-workflow-good-enough"),
    description: t("settings-decisions-whether-auto-goal-s-freshly-designed"),
  },
  "browser.headless": {
    label: t("settings-decisions-whether-page-needs-person-watching"),
    description: t("settings-decisions-under-unattended-browser-policy-whether-page"),
  },
  "workflow.judge": {
    label: t("settings-decisions-workflow-s-own-judge-step"),
    description: t("settings-decisions-step-drawn-judge-workflow-always-wherever"),
  },
  "agent.decide": {
    label: t("settings-decisions-agent-s-own-decide-tool"),
    description: t("settings-decisions-whether-agent-s-session-may-ask"),
  },
});

/** Every point id, in the order `POINTS` lists them. */
export const POINT_IDS = Object.freeze(Object.keys(POINTS));

/** A point's label, or the bare id when this build does not know it. */
export function pointLabel(point) {
  return POINTS[point]?.label ?? String(point ?? "");
}

/**
 * Which of a provider's own setting keys the panel shows, beside the
 * provider picker itself. A harness names its model and how hard that model
 * works — a level, never `auto`: the judge's own session asks nobody.
 * `rlcdAuth` only matters for `rlcd`, whose auth field is one of its own.
 * @param {string} provider
 * @param {string} [rlcdAuth]
 * @returns {string[]}
 */
export function fieldsFor(provider, rlcdAuth) {
  switch (provider) {
    case "harness":
      return [KEYS.harness.id, KEYS.harness.model, KEYS.harness.effort];
    case "agent":
      return [KEYS.agent.id];
    case "jev":
      return [KEYS.jev.model];
    case "rlcd":
      return [KEYS.rlcd.endpoint, KEYS.rlcd.model, KEYS.rlcd.auth];
    default:
      return [];
  }
}

/**
 * Whether the chosen provider takes an API key at all: Jev always, RLCD only
 * when its own auth is `bearer`; a harness or an agent's session never does.
 * @param {string} provider
 * @param {string} [rlcdAuth]
 */
export function takesKey(provider, rlcdAuth) {
  if (provider === "jev") return true;
  if (provider === "rlcd") return rlcdAuth === "bearer";
  return false;
}

/**
 * The `decisions.points_off` list after switching one point on or off. An
 * explicitly selected point (a strategy, an effort of `auto`, a provider
 * choice, a step kind) is never in this list — the switch that edits it is
 * disabled for one, so this is never asked to add it, but it is still
 * idempotent either way.
 * @param {readonly string[] | null | undefined} current
 * @param {string} point
 * @param {boolean} on
 */
export function pointsOffAfter(current, point, on) {
  const set = new Set(current ?? []);
  if (on) set.delete(point);
  else set.add(point);
  return [...set];
}

/** Whether a point is on: not in the off list, or selected explicitly (which cannot be switched off). */
export function pointIsOn(row) {
  if (!row) return false;
  return row.selected_explicitly || row.on;
}

/**
 * The list of points left to their own rule that a switch is built on: the
 * one this window wrote last, until the node's own read has caught up with
 * it — a second switch flipped before then would otherwise be built on the
 * list as it was read, and take the first one's change back. A value that
 * is no list is none.
 * @param {unknown} read `decisions.points_off` as the resolved settings say it
 * @param {readonly string[] | null | undefined} written what this window wrote and has not read back yet
 */
export function offList(read, written) {
  if (Array.isArray(written)) return [...written];
  return Array.isArray(read) ? read.filter((p) => typeof p === "string") : [];
}

/**
 * Every decision point as the panel draws it, in the node's order: its
 * words, whether it is on, whether it was selected by name where it is used
 * — on whatever the switches say — and whether its switch is **held**: a
 * point selected by name is never the panel's to switch; none is while the
 * workspace's own switch is off, while the status is unread, or while a
 * write of the list is on its way.
 * @param {{enabled?: boolean, points?: readonly object[]} | null | undefined} status
 * @param {string | null} writing the point whose switch was flipped and is being written, if any
 */
export function pointRows(status, writing) {
  const rows = status?.points ?? [];
  return POINT_IDS.map((id) => {
    const row = rows.find((r) => r.point === id) ?? null;
    const selected = Boolean(row?.selected_explicitly);
    const words = POINTS[id];
    return {
      id,
      label: words.label,
      hint: selected ? t("settings-decisions-panel-where-selected", { description: words.description }) : words.description,
      on: pointIsOn(row),
      selected,
      held: selected || writing !== null || !status?.enabled,
    };
  });
}

/**
 * The readiness line above the try-it box and the points list. The
 * workspace's switch is one of four ways the Decision-Making Agent is asked
 * (15 §Where it is on): off, a point selected by name — a `judge` step, an
 * `auto_route` plan, an effort of `auto`, the classifier's provider — and
 * an agent's or a workflow's own switch still ask. So *off* says where it
 * is still asked, and a provider that cannot be asked is said either way.
 */
export function readyLine(status) {
  if (!status) return { tone: "quiet", text: t("settings-decisions-reading-decision-making-agent") };
  const problem = status.problem || t("settings-decisions-ready");
  if (!status.enabled) {
    return status.ready
      ? { tone: "quiet", text: t("settings-decisions-off-asked-where-selected", { answers_as: status.answers_as }) }
      : { tone: "warn", text: t("settings-decisions-off-cannot-be-asked", { problem }) };
  }
  if (status.ready) return { tone: "ok", text: t("settings-decisions-ready-answers", { answers_as: status.answers_as }) };
  return { tone: "warn", text: problem };
}

/**
 * A provider's API key as its box holds it: what was typed, hidden, for as
 * long as the window lives, and what the node already holds of it (`sent`),
 * so Save waits for a change. **The box is its provider's own**: a key
 * typed for one provider is never drawn, revealed or sent under another.
 * @param {string} provider
 */
export function blankKey(provider) {
  return { provider, typed: "", sent: null };
}

/** The box as it stands for `provider`: its own, or an empty one when the box held another provider's key. */
export function keyFor(box, provider) {
  return box?.provider === provider ? box : blankKey(provider);
}

/** What was typed, under `provider`. */
export function keyTyped(box, provider, typed) {
  return { ...keyFor(box, provider), typed };
}

/** The node took the key: the box keeps it, and Save waits for a change. */
export function keySaved(box, provider) {
  const own = keyFor(box, provider);
  return { ...own, sent: own.typed };
}

/** The key was cleared on the node: the box is empty. */
export function keyCleared(provider) {
  return blankKey(provider);
}

/** Whether Save has something to send: a key typed under this provider that the node does not hold yet. */
export function maySaveKey(box, provider) {
  const own = keyFor(box, provider);
  return own.typed.trim() !== "" && own.typed !== own.sent;
}

/** Whether there is a key on the node to clear. */
export function mayClearKey(status) {
  return status?.key_stored === true;
}

/** Whether an engine fact moves the status: a `decisions.*` setting written, or a judgement recorded. */
export function movesStatus(payload) {
  if (payload?.type === "judged") return true;
  return payload?.type === "settings_changed" && Array.isArray(payload.keys) && payload.keys.some((k) => typeof k === "string" && k.startsWith("decisions.")); // for the machine
}

/** Whether an engine fact moves the recent judgements: one was recorded. */
export function movesJudgements(payload) {
  return payload?.type === "judged";
}

/** The note under a header whose model is not calibrated — never hidden, never alarming. */
export function calibratedNote(status) {
  if (!status || status.calibrated) return null;
  return t("settings-decisions-calibrated-probabilities-above-model-s-own");
}

/** A blank *Try it* form. */
export function blankTryForm() {
  return { state: "", kind: "noul", instructions: "", options: [{ id: "", meaning: "" }, { id: "", meaning: "" }] };
}

/** The options a `choice` form has actually filled in — blank rows are dropped. */
function filledOptions(options) {
  return (options ?? []).filter((o) => String(o.id ?? "").trim() && String(o.meaning ?? "").trim());
}

/** What stops a *Try it* form being sent, or `null`. */
export function tryProblem(form) {
  if (!String(form.instructions ?? "").trim()) return t("settings-decisions-give-question-instructions");
  if (form.kind === "choice" && filledOptions(form.options).length < 2) return t("settings-decisions-choice-needs-least-two-options");
  return null;
}

/**
 * The `DecisionRequest` a *Try it* form sends — one question named `q`, over
 * the state as typed. Nothing is decided and nothing is recorded by the
 * route this is sent to.
 */
export function tryRequest(form) {
  const instructions = String(form.instructions ?? "").trim();
  const question =
    form.kind === "choice"
      ? { type: "choice", instructions, criteria: Object.fromEntries(filledOptions(form.options).map((o) => [o.id.trim(), o.meaning.trim()])) }
      : { type: "noul", instructions };
  return { state: form.state ?? "", questions: { q: question } };
}

/** One answer, as a line — reused for a `try` response and a recorded judgement. */
function answerLine(id, a) {
  if (!a) return t("settings-decisions-answer-none", { id });
  const pct = (n) => percent(Number(n));
  switch (a.type) {
    case "noul": {
      // `noul` is a probability, not a plain yes/no — `certainty()` reads it
      // as `|p − 0.5| × 2`, so 0.5 is the least sure a noul answer can be.
      const p = Number(a.noul);
      const certainty = Math.abs(p - 0.5) * 2;
      return t("settings-decisions-answer-yes-no", { id, answer: p >= 0.5 ? "yes" : "no", sure: pct(certainty) });
    }
    case "choice":
      return t("settings-decisions-answer-choice", { id, choice: a.choice, sure: pct(a.confidence) });
    case "score":
      return a.legend?.[String(a.score)] ? t("settings-decisions-answer-score-legend", { id, score: a.score, legend: a.legend[String(a.score)], sure: pct(a.confidence) }) : t("settings-decisions-answer-score", { id, score: a.score, sure: pct(a.confidence) });
    default:
      return t("settings-decisions-answer-none", { id });
  }
}

/** Every answer of a `DecisionResponse`, one line each, in the order the questions were asked. */
export function answerLines(response) {
  const answers = response?.answers ?? {};
  return Object.entries(answers).map(([id, a]) => answerLine(id, a));
}

/**
 * One recorded judgement, as `GET /decisions` lists it: when, the point, the
 * outcome, the model, its answers in one line, and the reason for an
 * `unsure` or `failed` outcome.
 */
export function judgementLine(record, now = Date.now() / 1000) {
  const j = record?.judgement;
  if (!j) return "";
  const answers = Object.entries(j.answers ?? {})
    .map(([id, a]) => answerLine(id, a))
    .join(", ");
  const pieces = [agoWords(now - record.at), pointLabel(j.point), j.outcome, ...(answers ? [answers] : []), j.model];
  const line = pieces.join(" · ");
  return j.reason ? t("settings-decisions-line-reason", { line, reason: j.reason }) : line;
}

/**
 * The Agents screen's card line for the Decision-Making Agent: off, or on
 * with who currently answers for it. A status that could not be read says
 * so, with the reason — never *reading…* for good; the last answer stands
 * while a re-read fails.
 * @param {{enabled?: boolean, answers_as?: string} | null | undefined} status
 * @param {string | null} [error] why the status could not be read
 */
export function coreLine(status, error = null) {
  if (!status) return error ? t("settings-decisions-core-unread", { error }) : "";
  if (!status.enabled) return t("settings-decisions-core-off");
  return t("settings-decisions-workspace", { answers_as: status.answers_as });
}
