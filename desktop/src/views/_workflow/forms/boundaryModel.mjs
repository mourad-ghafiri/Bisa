/**
 * A step's boundary events as the designer edits and draws them
 * (03-workflows §Boundary events): which steps may carry them, a fresh one
 * of each event — a timeout, a reminder, a message, a signal — each either
 * **diverting** the step to a path of its own or **acting beside** it (a
 * post, a signal), a name no branch or other boundary of the step has, a
 * rename that carries the divert's flows along, a removal that takes them
 * away, and the words and glyph a chip on the card's lower edge wears.
 *
 * A reminder never diverts: a divert on a cadence would stop the step at its
 * first tick (`reminder_interrupts`), so its acts are the two beside the
 * step. A divert's flows are the step's flows labelled with its name — its
 * chip carries the handle they leave from (`branch:<name>`); an act has no
 * flows, so its chip is dashed and handle-less. Where each divert's handle
 * sits along the card's lower edge is one rule here, read by the card, the
 * thumbnail and the layout alike.
 *
 * Pure, so `node --test` reads it; `BoundaryEventsEditor.tsx` draws the
 * editor and `StepNode.tsx` the chips.
 */

import { duration } from "../../../i18n/format.mjs";
import { t } from "../../../i18n/l10n.mjs";
import { branchesOf, kindBranchesOf, mayCarryBoundaries } from "../stepKinds.mjs";

export { mayCarryBoundaries };

/** How many times a reminder fires when its `max` is not written — the core's `DEFAULT_REMINDERS`. */
export const DEFAULT_REMINDERS = 3;

/** A day, in seconds: a new timeout's and a new reminder's clock. */
export const DAY_SECS = 86400;

/** The name a new boundary of each event starts from. */
const NAME_ROOT = Object.freeze({ after: "timeout", every: "reminder", message: "message", signal: "signal" });

/** The acts an event may take, in the editor's order: a reminder never diverts. */
export function actsFor(event) {
  return event === "every" ? ["notify", "emit"] : ["divert", "notify", "emit"];
}

/** Every name a step's flows or boundaries already use — its kind's branches, and each boundary's name. */
function takenNames(step) {
  return new Set([...branchesOf(step), ...(step?.boundaries ?? []).map((b) => b?.name).filter(Boolean)]);
}

/** A name the step has for none of its branches or boundaries: `timeout`, `timeout-2`, … */
export function freshBoundaryName(step, root) {
  const taken = takenNames(step);
  if (!taken.has(root)) return root;
  for (let n = 2; ; n++) {
    const name = `${root}-${n}`;
    if (!taken.has(name)) return name;
  }
}

/** A fresh `on` of one event. */
export function blankOn(event) {
  switch (event) {
    case "after":
      return { event, secs: DAY_SECS };
    case "every":
      return { event, secs: DAY_SECS, max: DEFAULT_REMINDERS };
    case "message":
      return { event };
    case "signal":
      return { event, name: "" };
    default:
      throw new Error(`not a boundary event: ${event}`);
  }
}

/** An act and its own fields, fresh: a divert has none, a post its words, an emit its signal. */
export function blankAct(act) {
  switch (act) {
    case "divert":
      return { act };
    case "notify":
      return { act, template: "" };
    case "emit":
      return { act, signal: "" };
    default:
      throw new Error(`not a boundary act: ${act}`);
  }
}

/**
 * A fresh boundary of one event for `step`: a timeout, a message and a
 * signal divert — the common case, a path of their own — and a reminder
 * posts beside the step.
 */
export function blankBoundary(step, event) {
  const act = event === "every" ? "notify" : "divert";
  return { name: freshBoundaryName(step, NAME_ROOT[event] ?? "boundary"), on: blankOn(event), ...blankAct(act) };
}

/** The boundary named `name` on `step`, or `null`. */
export function boundaryOf(step, name) {
  return (step?.boundaries ?? []).find((b) => b.name === name) ?? null;
}

/** The step with a fresh boundary of `event` added at the end. */
export function addBoundary(step, event) {
  return { ...step, boundaries: [...(step.boundaries ?? []), blankBoundary(step, event)] };
}

/** The step without the flows labelled `name` — a divert's path, gone with it. */
function withoutFlowsOf(step, name) {
  return { ...step, then: (step.then ?? []).filter((f) => f.branch !== name) };
}

/** One boundary replaced wholesale — what a field's edit writes. Its name is renamed only through `renameBoundaryOn`. */
export function replaceBoundary(step, name, next) {
  return { ...step, boundaries: (step.boundaries ?? []).map((b) => (b.name === name ? { ...next, name } : b)) };
}

/**
 * The event changed: a fresh `on` of the new kind. A reminder cannot divert,
 * so a divert that becomes one posts instead — and its flows go, since a
 * post has none.
 */
export function setEvent(step, name, event) {
  const b = boundaryOf(step, name);
  if (!b || b.on?.event === event) return step;
  const stays = actsFor(event).includes(b.act);
  const act = stays ? pickAct(b) : blankAct(actsFor(event)[0]);
  const next = replaceBoundary(step, name, { name, on: blankOn(event), ...act });
  return stays ? next : withoutFlowsOf(next, name);
}

/** A boundary's act and its own fields, as written. */
function pickAct(b) {
  switch (b.act) {
    case "notify":
      return { act: "notify", template: b.template ?? "", ...(b.scope ? { scope: b.scope } : {}), ...(b.mentions?.length ? { mentions: b.mentions } : {}), ...(b.author ? { author: b.author } : {}) };
    case "emit":
      return { act: "emit", signal: b.signal ?? "", ...(b.payload && Object.keys(b.payload).length > 0 ? { payload: b.payload } : {}) };
    default:
      return { act: "divert" };
  }
}

/**
 * The act changed: fresh fields for the new act. A divert that becomes an
 * act beside the step loses its flows — nothing takes a path an act never
 * opens. Refused for a reminder asked to divert: the step keeps its act.
 */
export function setAct(step, name, act) {
  const b = boundaryOf(step, name);
  if (!b || b.act === act || !actsFor(b.on?.event).includes(act)) return step;
  const next = replaceBoundary(step, name, { name, on: b.on, ...blankAct(act) });
  return b.act === "divert" ? withoutFlowsOf(next, name) : next;
}

/**
 * Rename a boundary, carrying a divert's flows along — every flow labelled
 * with the old name takes the new one, so a rename never orphans a path.
 * Refused, with the reason, when the name is empty, too long, or another
 * branch or boundary of the step has it.
 * @returns {{ok: true, step: object} | {ok: false, reason: string}}
 */
export function renameBoundaryOn(step, from, to) {
  const next = (to ?? "").trim();
  if (next === from) return { ok: true, step };
  if (!next) return { ok: false, reason: t("workflow-boundary-model-needs-name") };
  if (next.length > 64) return { ok: false, reason: t("workflow-boundary-model-name-too-long") };
  if (!boundaryOf(step, from)) return { ok: false, reason: t("workflow-boundary-model-no-such-boundary", { name: from }) };
  if (takenNames(step).has(next)) return { ok: false, reason: t("workflow-boundary-model-name-taken", { name: next }) };
  return {
    ok: true,
    step: {
      ...step,
      boundaries: (step.boundaries ?? []).map((b) => (b.name === from ? { ...b, name: next } : b)),
      then: (step.then ?? []).map((f) => (f.branch === from ? { ...f, branch: next } : f)),
    },
  };
}

/** Remove a boundary, and a divert's flows with it. */
export function removeBoundaryOn(step, name) {
  const b = boundaryOf(step, name);
  if (!b) return step;
  const next = { ...step, boundaries: (step.boundaries ?? []).filter((x) => x.name !== name) };
  return b.act === "divert" ? withoutFlowsOf(next, name) : next;
}

/** Where a step's plain `out` handle sits along its lower edge once diverts share the edge, in percent. */
export const OUT_WITH_DIVERTS = 30;

/** The span of the lower edge a step's divert handles spread across, in percent: its right side. */
const DIVERT_SPAN = Object.freeze([55, 95]);

/**
 * Where each divert's handle sits along the card's lower edge, in percent,
 * in declaration order: spread across the edge's right side, so a timeout's
 * path leaves off the card's lower right and the normal flow keeps the left.
 * @returns {Map<string, number>}
 */
export function divertOffsets(step) {
  const diverts = (step?.boundaries ?? []).filter((b) => b?.act === "divert" && b.name);
  const out = new Map();
  const [lo, hi] = DIVERT_SPAN;
  diverts.forEach((b, i) => {
    out.set(b.name, diverts.length === 1 ? (lo + hi) / 2 : lo + ((hi - lo) * i) / (diverts.length - 1));
  });
  return out;
}

/**
 * Where a step's own flows leave its lower edge, in percent: its kind's
 * branches spread across the whole edge — a gateway carries no diverts —
 * else its one `out`, at the centre, or at the left once diverts share the
 * edge.
 * @returns {{out: number | null, branches: Map<string, number>}}
 */
export function ownOffsets(step) {
  const kind = kindBranchesOf(step);
  if (kind.length > 0) return { out: null, branches: new Map(kind.map((b, i) => [b, ((i + 1) / (kind.length + 1)) * 100])) };
  return { out: divertOffsets(step).size > 0 ? OUT_WITH_DIVERTS : 50, branches: new Map() };
}

/** A clock's seconds in words: a fixed span as a person reads it, an input by its name. */
function secsWords(secs) {
  if (secs && typeof secs === "object" && "input" in secs) return `{inputs.${secs.input}}`;
  return duration(Number(secs) || 0);
}

/**
 * What a chip on the card's lower edge says and wears: the event's glyph
 * key (`ui/icons`' `BOUNDARY_ON_ICON`), the act's when it acts beside the
 * step, the short words — *after 2d*, *every 1d*, *message*, *signal
 * report.ready* — whether it diverts (a handle, a solid border) and the
 * boundary's name, the label its flows carry.
 */
export function chipOf(boundary) {
  const on = boundary?.on ?? {};
  const event = on.event ?? "after";
  let words;
  switch (event) {
    case "after":
      words = t("workflow-boundary-model-after", { secs: secsWords(on.secs) });
      break;
    case "every":
      words = t("workflow-boundary-model-every", { secs: secsWords(on.secs), max: on.max ?? DEFAULT_REMINDERS });
      break;
    case "message":
      words = on.contains ? t("workflow-boundary-model-message-containing", { text: on.contains }) : t("workflow-boundary-model-message");
      break;
    case "signal":
      words = on.name ? t("workflow-boundary-model-signal-named", { name: on.name }) : t("workflow-boundary-model-signal");
      break;
    default:
      words = String(event);
  }
  const diverts = boundary?.act === "divert";
  return { name: boundary?.name ?? "", event, act: boundary?.act ?? "divert", diverts, words };
}

/**
 * What the editor says under one boundary: where a divert leads — the steps
 * its flows reach, or that none does yet — or what an act does beside the
 * live step.
 * @param {object} step
 * @param {object} boundary
 */
export function consequence(step, boundary) {
  if (boundary.act === "divert") {
    const to = (step.then ?? []).filter((f) => f.branch === boundary.name).map((f) => f.to);
    return to.length > 0 ? t("workflow-boundary-model-diverts-to", { steps: to.join(", ") }) : t("workflow-boundary-model-diverts-nowhere-yet", { name: boundary.name });
  }
  return boundary.act === "notify" ? t("workflow-boundary-model-posts-beside") : t("workflow-boundary-model-emits-beside");
}
