/**
 * Who hears a workflow's start events, as the screens say it (03-workflows
 * §Listening). A library workflow hears them only once a person turns it
 * **On** — the designer's header switch, the library card, the Turn on
 * dialog — and a goal whose workflow begins on events **listens** while it
 * is open. Both keep a `Listening`: the inputs its event runs bind, a
 * per-run budget, since when, and why it paused.
 *
 * The switch reads one of five states from the node's `WorkflowRow`
 * (`listening`, `starts`, `listening_needs`, `problems`) and its listeners:
 * nothing to hear (no switch); *Off*; *Can't turn on: 2 problems*; *On —
 * listening for …, next Mon 09:00*; *Paused: a run it started failed*. The
 * goal page's header line reads a goal's `Listening` the same way. Pure,
 * so `node --test` reads it; `ListeningSwitch.tsx`, `TurnOnDialog.tsx` and
 * `GoalHeader.tsx` draw it.
 */

import { dateTime } from "../../i18n/format.mjs";
import { listeningInputs } from "./forms/startForm.mjs";
import { t, tx } from "../../i18n/l10n.mjs";

/** How a next occurrence reads: its weekday and its time, in this machine's zone. */
export const NEXT_DUE_FORMAT = Object.freeze({ weekday: "short", hour: "2-digit", minute: "2-digit" });

/** The row's starts that begin on an event — what the workflow listens for. */
export function eventStartsOf(row) {
  return (row?.starts ?? []).filter((s) => s?.event && s.event !== "manual");
}

/** Whether the workflow has anything to hear: a start that begins on an event. */
export function hasEventStarts(row) {
  return eventStartsOf(row).length > 0;
}

/** What it listens for, in the node's words — each event start's summary, joined. */
export function listeningFor(row) {
  return eventStartsOf(row)
    .map((s) => tx(s.summary))
    .filter(Boolean)
    .join(" · ");
}

/** The soonest a timed or polled listener comes due, unix seconds; `null` when none keeps a clock. */
export function nextDue(listeners) {
  let best = null;
  for (const l of listeners ?? []) {
    if (typeof l?.next_due === "number" && (best === null || l.next_due < best)) best = l.next_due;
  }
  return best;
}

/** A next occurrence in words: *Mon 09:00*. */
export function nextWords(at) {
  return dateTime(at, NEXT_DUE_FORMAT);
}

/** Why a host paused, in words. */
export function pausedWords(paused) {
  switch (paused?.reason?.reason) {
    case "run_failed":
      return t("workflow-listening-model-paused-run-failed");
    case "budget_spent":
      return t("workflow-listening-model-paused-budget-spent");
    default:
      return t("workflow-listening-model-paused-unknown");
  }
}

/**
 * What stops a person turning a workflow On, in words: put away, a goal's
 * own design (its goal listens instead), problems, a step that reads the
 * goal it serves — an event would start a run in the workspace, which has
 * none (`workspace_problems`, the node's `needs_goal`). Empty when nothing
 * does.
 */
export function turnOnBlockers(row) {
  const out = [];
  if (!row) return out;
  if (row.workflow?.archived) out.push(t("workflow-listening-model-archived"));
  if (row.workflow?.origin?.origin === "goal") out.push(t("workflow-listening-model-goal-design"));
  const n = (row.problems ?? []).length;
  if (n > 0) out.push(t("workflow-listening-model-problems", { n }));
  if ((row.workspace_problems ?? []).length > 0) out.push(t("workflow-listening-model-reads-goal"));
  return out;
}

/**
 * The switch, from the row and its listeners: which state, its words, the
 * tone it wears, and what pressing it does — `on` (the Turn on dialog),
 * `off`, or nothing. A paused workflow may also be heard again (`again`).
 * @returns {{state: "none" | "off" | "blocked" | "on" | "paused", words: string, tone: "quiet" | "ok" | "warn" | "danger", toggle: "on" | "off" | null, again: boolean}}
 */
export function switchState(row, listeners = []) {
  if (!hasEventStarts(row)) return { state: "none", words: "", tone: "quiet", toggle: null, again: false };
  const listening = row.listening ?? null;
  if (listening?.paused) {
    return { state: "paused", words: t("workflow-listening-model-paused", { why: pausedWords(listening.paused) }), tone: "warn", toggle: "off", again: true };
  }
  if (listening) {
    const what = listeningFor(row);
    const next = nextDue(listeners);
    return {
      state: "on",
      words: next === null ? t("workflow-listening-model-on", { what }) : t("workflow-listening-model-on-next", { what, next: nextWords(next) }),
      tone: "ok",
      toggle: "off",
      again: false,
    };
  }
  const blockers = turnOnBlockers(row);
  if (blockers.length > 0) return { state: "blocked", words: t("workflow-listening-model-cannot-turn-on", { why: blockers.join(" · ") }), tone: "danger", toggle: null, again: false };
  return { state: "off", words: t("workflow-listening-model-off"), tone: "quiet", toggle: "on", again: false };
}

/**
 * The inputs turning On asks — the ones its event starts read and do not
 * map, with no default (`listening_needs`, the node's word) — as the
 * workflow declares them, in its order, each one required: the node refuses
 * a turn-on that leaves one out. An input a default fills is never asked,
 * whatever the row names (`startForm.listeningInputs`).
 */
export function neededInputs(row) {
  return listeningInputs(row?.workflow, row?.listening_needs ?? []);
}

/** The budget fields a per-run ceiling may set. */
const BUDGET_KEYS = Object.freeze(["max_usd_cents", "max_tokens", "max_wall_clock_secs"]);

/**
 * What `PUT …/listening` carries: the inputs asked, when there are any, and
 * a per-run budget when one was set — a blank or zero ceiling is none, and
 * the workspace default applies.
 */
export function turnOnBody(inputs, budget) {
  const body = {};
  if (inputs && Object.keys(inputs).length > 0) body.inputs = inputs;
  const b = {};
  for (const k of BUDGET_KEYS) {
    const v = Number(budget?.[k]);
    if (Number.isFinite(v) && v > 0) b[k] = Math.trunc(v);
  }
  if (Object.keys(b).length > 0) body.budget = b;
  return body;
}

/** A number as a person types it: a comma is a decimal mark. `NaN` for what is none. */
function typed(text) {
  const s = String(text ?? "").trim().replace(",", ".");
  return s === "" ? Number.NaN : Number(s);
}

/**
 * A per-run ceiling as the Turn on dialog's three fields hold it — dollars,
 * tokens, minutes — read into the node's budget: cents, tokens, seconds. A
 * blank field is no ceiling. What is typed and is no ceiling — a word, zero,
 * a negative, less than a cent or a second, a fraction of a token — is
 * **said**, under its field: read as none it would start every run with no
 * ceiling of its own, unseen.
 * @param {{dollars?: string, tokens?: string, minutes?: string} | null | undefined} draft
 * @returns {{budget: {max_usd_cents: number | null, max_tokens: number | null, max_wall_clock_secs: number | null}, errors: Record<string, string>}}
 */
export function readBudget(draft) {
  const budget = { max_usd_cents: null, max_tokens: null, max_wall_clock_secs: null };
  const errors = {};
  const read = (field, key, scale, whole) => {
    const text = String(draft?.[field] ?? "").trim();
    if (text === "") return;
    const n = typed(text);
    if (!Number.isFinite(n) || n <= 0) {
      errors[field] = t("workflow-listening-model-budget-not-a-ceiling");
      return;
    }
    if (whole && !Number.isInteger(n)) {
      errors[field] = t("workflow-listening-model-budget-tokens-whole");
      return;
    }
    const value = Math.round(n * scale);
    if (value < 1) errors[field] = t("workflow-listening-model-budget-not-a-ceiling");
    else budget[key] = value;
  };
  read("dollars", "max_usd_cents", 100, false);
  read("tokens", "max_tokens", 1, true);
  read("minutes", "max_wall_clock_secs", 60, false);
  return { budget, errors };
}

/** What listening again sends: the inputs and the budget the host listened with before. */
export function againBody(listening) {
  return turnOnBody(listening?.inputs ?? {}, listening?.budget ?? null);
}

/**
 * A goal's listening line, for its header: *Listening — begins on the
 * schedule … · next Mon 09:00*, or *Paused: a run it started failed*.
 * `null` for a goal that does not listen. The verb beside it — *Stop
 * listening*, *Listen again* — is the goal's (`runControl.listenVerb`).
 * @returns {{paused: boolean, words: string, tone: "accent" | "warn"} | null}
 */
export function goalListening(listening, listeners = []) {
  if (!listening) return null;
  if (listening.paused) return { paused: true, words: t("workflow-listening-model-paused", { why: pausedWords(listening.paused) }), tone: "warn" };
  const what = (listeners ?? [])
    .map((l) => tx(l.summary))
    .filter(Boolean)
    .join(" · ");
  const next = nextDue(listeners);
  const words = !what
    ? t("workflow-listening-model-goal-listening")
    : next === null
      ? t("workflow-listening-model-goal-listening-for", { what })
      : t("workflow-listening-model-goal-listening-next", { what, next: nextWords(next) });
  return { paused: false, words, tone: "accent" };
}

/** Whether an engine event says this host's listening moved — what the switch and the header re-read on. */
export function movesListeningOf(event, host) {
  const p = event?.payload;
  if (!p) return false;
  if (p.type === "listening_changed") return p.host === host;
  if (p.type === "listener_failed" || p.type === "listener_fired") return typeof p.listener === "string" && p.listener.startsWith(`${host}/`);
  return false;
}
