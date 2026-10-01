/**
 * A `start` step as the designer edits it (03-workflows §Start events): the
 * event it begins on — by hand, a schedule's cadence, a call, a message, a
 * signal, a project's change, a run's end, a platform topic, an outside
 * platform's poll, a check's result — with a fresh set of fields for each;
 * its mapping, *Inputs from the event* (an input ← `{event.payload.…}`, the
 * one place the event is read); its guard, in words; a hook's local call;
 * and a test run's sample payload. And the workflow's ways in, read the
 * core's way: its starts, the one by hand (the manual entry) — none when
 * only events begin it — and what turning it On asks (`listening_needs`).
 *
 * Pure, so `node --test` reads it; `StartStepForm.tsx` draws the form, and
 * the switch, the dialogs and the cards read the rest.
 */

import { duration } from "../../../i18n/format.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** The event of a start by hand. */
export const MANUAL = "manual";

/** A new schedule's cadence, and a poll's or a check's: an hour, five minutes. */
export const DEFAULT_EVERY_SECS = 3600;
export const DEFAULT_POLL_SECS = 300;
/** A new cron cadence: 9 in the morning, Monday. */
export const DEFAULT_CRON = "0 9 * * 1";

/** The event a start step begins on — by hand for a step that names none. */
export function eventOf(step) {
  return step?.on?.event ?? MANUAL;
}

/**
 * A fresh `on` of one event, with the fields that event needs. A reference
 * not chosen yet — a project, a connector, its operation — is `null`, never
 * `""`; a name or a topic not written yet is `""`, which the validator names.
 */
export function blankStartOn(event) {
  switch (event) {
    case "manual":
      return { event };
    case "schedule":
      return { event, every: DEFAULT_EVERY_SECS };
    case "hook":
      return { event };
    case "message":
      return { event };
    case "signal":
      return { event, name: "" };
    case "project":
      return { event, project: null, change: "commit" };
    case "run":
      return { event };
    case "platform":
      return { event, topic: "" };
    case "connector":
      return { event, connector: null, operation: null, every: DEFAULT_POLL_SECS };
    case "check":
      return { event, command: "", fire_on: "starts_failing", every: DEFAULT_POLL_SECS };
    default:
      throw new Error(`not a start event: ${event}`);
  }
}

/**
 * The step with its event changed: a fresh `on` of the new event. A start
 * by hand maps nothing and guards nothing (`manual_start_configured`), so
 * its mapping and guard go with the change.
 */
export function setStartEvent(step, event) {
  if (eventOf(step) === event) return step;
  const next = { ...step, on: blankStartOn(event) };
  if (event === MANUAL) {
    delete next.inputs;
    delete next.guard;
  }
  return next;
}

/** Whether a schedule — a start's own, a poll's, a check's — keeps a cron cadence rather than seconds. */
export function cadenceOf(on) {
  return on?.cron !== undefined && on?.cron !== null ? "cron" : "every";
}

/** The `on` with its cadence switched: seconds, or a cron expression and its zone — exactly one of the two. */
export function setCadence(on, cadence) {
  const { every: _every, cron: _cron, tz: _tz, ...rest } = on;
  return cadence === "cron" ? { ...rest, cron: DEFAULT_CRON, tz: null } : { ...rest, every: DEFAULT_EVERY_SECS };
}

/**
 * What each event's occurrence carries: the fields a mapping may read as
 * `{event.payload.<field>}`, in the order the guide's table lists them
 * (`docs/guide/events.md` §Start events) — the engine's own payloads, held
 * equal by the test. A hook's body and a signal's payload are the caller's
 * own shape — their fields are written by hand. An occurrence's moment is
 * the event's own (`{event.at}`), never a field of its payload.
 */
export const PAYLOAD_FIELDS = Object.freeze({
  manual: [],
  schedule: ["at"],
  hook: [],
  message: ["message", "scope", "author", "author_kind", "teams", "mentions", "text"],
  signal: [],
  project: ["project", "slug", "change", "branch", "before", "after", "forced", "commits", "paths", "pull_request"],
  run: ["run", "workflow", "outcome", "goal"],
  platform: ["event", "fields", "goal", "workflow", "run"],
  connector: ["id", "item"],
  check: ["exit_code", "passed", "output", "command"],
});

/** A payload field as the template a mapping writes: `{event.payload.text}`. */
export function payloadTemplate(field) {
  return `{event.payload.${field}}`;
}

/** What an input's mapping may be picked from: the event's fields as templates. */
export function mappingSuggestions(event) {
  return (PAYLOAD_FIELDS[event] ?? []).map(payloadTemplate);
}

/**
 * *Inputs from the event*: one row per input the workflow declares — its
 * name, its label, whether a run needs it (required, with no default), and
 * the template the start maps onto it, `null` for none.
 */
export function mappingRows(step, inputs) {
  const mapping = step?.inputs ?? {};
  return (inputs ?? []).map((i) => ({
    input: i.name,
    label: i.label || i.name,
    required: !!i.required && (i.default === undefined || i.default === null),
    template: typeof mapping[i.name] === "string" ? mapping[i.name] : null,
  }));
}

/** The step with `input` mapped onto `template`, or unmapped when the template is blank; an empty mapping is no mapping. */
export function setMapping(step, input, template) {
  const mapping = { ...(step.inputs ?? {}) };
  if ((template ?? "").trim()) mapping[input] = template;
  else delete mapping[input];
  const next = { ...step };
  if (Object.keys(mapping).length > 0) next.inputs = mapping;
  else delete next.inputs;
  return next;
}

/** The inputs a mapping names that the workflow no longer declares — shown so they can be removed, never silently dropped. */
export function strayMappings(step, inputs) {
  const names = new Set((inputs ?? []).map((i) => i.name));
  return Object.keys(step?.inputs ?? {}).filter((k) => !names.has(k));
}

/** The guard as the form reads it: the overlap's word, how many at once when several, the debounce in seconds. */
export function guardOf(step) {
  const g = step?.guard ?? {};
  const o = g.overlap ?? "queue";
  const several = typeof o === "object" && o !== null && "parallel" in o;
  return {
    overlap: several ? "parallel" : o === "skip" ? "skip" : "queue",
    max: several ? Math.max(1, Math.trunc(Number(o.parallel)) || 1) : 2,
    debounce: Math.max(0, Math.trunc(Number(g.debounce_secs) || 0)),
  };
}

/**
 * The step with its guard set. The defaults — one at a time, no debounce —
 * are left unwritten, so a default guard is no guard at all, as the core
 * writes it.
 */
export function setGuard(step, { overlap, max, debounce }) {
  const guard = {};
  const secs = Math.max(0, Math.trunc(Number(debounce) || 0));
  if (secs > 0) guard.debounce_secs = secs;
  if (overlap === "skip") guard.overlap = "skip";
  else if (overlap === "parallel") guard.overlap = { parallel: Math.max(1, Math.trunc(Number(max)) || 1) };
  const next = { ...step };
  if (Object.keys(guard).length > 0) next.guard = guard;
  else delete next.guard;
  return next;
}

/** The guard in one sentence: what an occurrence does while a run this start began is still going, and the debounce. */
export function guardWords(step) {
  const g = guardOf(step);
  const overlap =
    g.overlap === "skip" ? t("workflow-start-form-guard-skip") : g.overlap === "parallel" ? t("workflow-start-form-guard-parallel", { max: g.max }) : t("workflow-start-form-guard-queue");
  return g.debounce > 0 ? t("workflow-start-form-guard-debounced", { overlap, secs: duration(g.debounce) }) : overlap;
}

/** A value reference in words: an input by its placeholder, a fixed value as it is. */
function refWords(v) {
  return v && typeof v === "object" && "input" in v ? `{inputs.${v.input}}` : String(v ?? "");
}

/** Seconds in words: an input by its placeholder, a fixed span as a person reads it. */
function secsWords(v) {
  return v && typeof v === "object" && "input" in v ? `{inputs.${v.input}}` : duration(Number(v) || 0);
}

/**
 * An event in a few words — *by hand*, *every 1h*, *on `0 9 * * 1`*, *when
 * called from outside*, *a message in support*, *the signal report.ready* —
 * what a card's second line and a switch's line say before the node has
 * judged the saved design.
 */
export function eventPhrase(on) {
  switch (on?.event ?? MANUAL) {
    case "manual":
      return t("workflow-start-form-by-hand");
    case "schedule":
      if (on.cron !== undefined && on.cron !== null) return t("workflow-start-form-on-cron", { cron: refWords(on.cron) });
      if (on.every !== undefined && on.every !== null) return t("workflow-start-form-every", { secs: secsWords(on.every) });
      return t("workflow-start-form-schedule-unset");
    case "hook":
      return on.public ? t("workflow-start-form-called-from-outside") : t("workflow-start-form-when-called");
    case "message":
      return on.in ? t("workflow-start-form-message-in", { scope: on.in }) : t("workflow-start-form-message");
    case "signal":
      return on.name ? t("workflow-start-form-signal-named", { name: on.name }) : t("workflow-start-form-signal");
    case "project":
      return t("workflow-start-form-project-change", { change: on.change ?? "commit" });
    case "run":
      return t("workflow-start-form-run-ends", { outcome: on.outcome ?? "any" });
    case "platform":
      return on.topic ? t("workflow-start-form-platform-topic", { topic: on.topic }) : t("workflow-start-form-platform");
    case "connector":
      return on.connector && on.operation ? t("workflow-start-form-poll-call", { call: `${on.connector}.${on.operation}` }) : t("workflow-start-form-poll");
    case "check":
      return t("workflow-start-form-check", { fire_on: on.fire_on ?? "starts_failing" });
    default:
      return String(on.event);
  }
}

/**
 * A hook start's local call, under the control-plane token — a library
 * workflow's, or a goal's. The public door (`POST /hooks/<host>/<step>`)
 * exists only when the start is public and the machine allows it; the
 * listener names it once armed.
 * @param {{workflow?: string | null, goal?: string | null}} host
 * @param {string} step
 */
export function localHookPath(host, step) {
  return host?.goal ? `/goals/${host.goal}/hooks/${step}` : `/workflows/${host?.workflow ?? ""}/hooks/${step}`;
}

/** A value as a sample shows it: what is written, never a reference to an input nobody has bound yet. */
function fixed(v, fallback) {
  return typeof v === "string" && v !== "" ? v : fallback;
}

/**
 * A test run's sample payload for `on` — **every** field its occurrences
 * carry (`PAYLOAD_FIELDS`), so a mapping picked from the form's list finds
 * its value, with what the start's own filter demands already filled, so
 * the sample reads as an occurrence the start would hear. `null` for a
 * start by hand, which a test run never names.
 * @param {object} on
 * @param {number} [now] unix seconds
 */
export function samplePayload(on, now = Math.floor(Date.now() / 1000)) {
  switch (on?.event ?? MANUAL) {
    case "manual":
      return null;
    case "schedule":
      return { at: now };
    case "hook":
      return {};
    case "message":
      return { message: "", scope: fixed(on.in, ""), author: "", author_kind: on.from === "agents" ? "agent" : "you", teams: [], mentions: [], text: on.contains ?? "" };
    case "signal":
      return { ...(on.fields ?? {}) };
    case "project":
      return { project: fixed(on.project, ""), slug: "", change: on.change ?? "commit", branch: fixed(on.branch, ""), before: "", after: "", forced: false, commits: [], paths: [], pull_request: null };
    case "run":
      return { run: "", workflow: fixed(on.workflow, ""), outcome: on.outcome ?? "failed", goal: null };
    case "platform":
      return { event: on.topic ?? "", fields: { ...(on.fields ?? {}) }, goal: null, workflow: null, run: null };
    case "connector":
      return { id: "", item: {} };
    case "check":
      return { exit_code: 1, passed: false, output: "", command: on.command ?? "" };
    default:
      return {};
  }
}

/** The names a template reads as `{inputs.<name>…}` — a doubled brace is the author's text, not a placeholder. */
export function inputPlaceholders(tmpl) {
  if (typeof tmpl !== "string") return [];
  const bare = tmpl.split("{{").join("").split("}}").join("");
  return [...new Set([...bare.matchAll(/\{inputs\.([a-z][a-z0-9_-]*)/g)].map((m) => m[1]))];
}

/** The inputs a start's event reads through a reference: a cadence's seconds or cron, who wrote or is mentioned, a project, an account. */
export function startInputRefs(on) {
  const out = [];
  const ref = (v) => {
    if (v && typeof v === "object" && "input" in v && typeof v.input === "string") out.push(v.input);
  };
  if (!on) return out;
  ref(on.every);
  ref(on.cron);
  ref(on.from);
  ref(on.mentions);
  ref(on.project);
  ref(on.account);
  return [...new Set(out)];
}

/** Every template a start's event fields carry — read against the inputs it listens with, never the run. */
export function startTemplates(on) {
  if (!on) return [];
  const values = (m) => Object.values(m ?? {}).filter((v) => typeof v === "string");
  switch (on.event) {
    case "message":
      return [on.in, on.contains].filter((v) => typeof v === "string");
    case "signal":
      return [on.name, ...values(on.fields)].filter((v) => typeof v === "string");
    case "project":
      return [on.branch, on.glob].filter((v) => typeof v === "string");
    case "platform":
      return values(on.fields);
    case "connector":
      return values(on.params);
    case "check":
      return typeof on.command === "string" ? [on.command] : [];
    default:
      return [];
  }
}

/**
 * The ways a run may begin: the `start` steps, in display order — or, in a
 * workflow that names none, its root, the step nothing flows or fails into
 * (else the first). The core's `start_steps`.
 */
export function startSteps(wf) {
  const steps = wf?.steps ?? [];
  const named = steps.filter((s) => s.kind === "start");
  if (named.length > 0) return named;
  const targets = new Set(steps.flatMap((s) => [...(s.then ?? []).map((f) => f.to), ...(s.on_fail?.on_fail === "then" && s.on_fail.step ? [s.on_fail.step] : [])]));
  const roots = steps.filter((s) => !targets.has(s.id));
  return roots.length > 0 ? roots : steps.slice(0, 1);
}

/**
 * Where a run by hand begins: the start by hand, else the one root of a
 * workflow that names no start; `null` for a workflow only events begin, or
 * one whose roots are ambiguous. The core's `manual_entry`.
 */
export function manualEntry(wf) {
  const steps = wf?.steps ?? [];
  if (steps.some((s) => s.kind === "start")) return steps.find((s) => s.kind === "start" && eventOf(s) === MANUAL) ?? null;
  const roots = startSteps(wf);
  return roots.length === 1 ? roots[0] : null;
}

/** Every start that begins on an event, in display order. */
export function eventStarts(wf) {
  return (wf?.steps ?? []).filter((s) => s.kind === "start" && eventOf(s) !== MANUAL);
}

/** Whether a start begins on an event: a library workflow a person turns On, a goal's design its goal listens with. */
export function listens(wf) {
  return eventStarts(wf).length > 0;
}

/**
 * What a host must give when it begins listening: for every event start,
 * the required inputs its mapping does not supply, and the inputs its
 * event's fields read — each only when no default fills it. The union over
 * the starts, since any of them may begin a run; in the inputs' own order.
 * The core's `listening_needs`.
 */
export function listeningNeeds(wf) {
  const inputs = wf?.inputs ?? [];
  const unfilled = (name) => inputs.some((d) => d.name === name && (d.default === undefined || d.default === null));
  const out = new Set();
  for (const s of eventStarts(wf)) {
    const mapping = s.inputs ?? {};
    for (const d of inputs) {
      if (d.required && (d.default === undefined || d.default === null) && !(d.name in mapping)) out.add(d.name);
    }
    for (const name of startInputRefs(s.on)) if (unfilled(name)) out.add(name);
    for (const tmpl of startTemplates(s.on)) for (const name of inputPlaceholders(tmpl)) if (unfilled(name)) out.add(name);
  }
  return inputs.map((d) => d.name).filter((n) => out.has(n));
}

/**
 * The inputs a host is asked when it begins listening, as the form draws
 * them: the ones `needs` names, in the workflow's own order, each one
 * **required** — the node refuses a host that leaves one of them out,
 * whatever the input says of itself for a run by hand (an optional input an
 * event's own field reads has to be given all the same). An input a default
 * fills is never among them: nobody gave it, and it is read at its default.
 * `needs` is the node's word when a row carries it (`listening_needs`),
 * else this build's own reading of the definition.
 * @param {{inputs?: object[]} | null | undefined} wf
 * @param {readonly string[]} [needs]
 */
export function listeningInputs(wf, needs = listeningNeeds(wf)) {
  const asked = new Set(needs ?? []);
  return (wf?.inputs ?? [])
    .filter((d) => asked.has(d.name) && (d.default === undefined || d.default === null))
    .map((d) => (d.required === true ? d : { ...d, required: true }));
}

/** No inputs — one list, so a screen that memoises on identity sees the same nothing at every render. */
const NO_INPUTS = Object.freeze([]);

/**
 * The inputs a start asks: every input for a run by hand — the definition's
 * own list, untouched — and what listening needs when the start begins by
 * listening.
 * @param {{inputs?: object[]} | null | undefined} wf
 * @param {boolean} listen the start arms the design's start events
 */
export function startInputs(wf, listen) {
  return listen ? listeningInputs(wf) : (wf?.inputs ?? NO_INPUTS);
}

/** A signal's name: dotted words of `a-z`, `0-9`, `_` and `-` — the core's `valid_signal_name`. */
export function validSignalName(name) {
  return (
    typeof name === "string" &&
    name.length > 0 &&
    name.length <= 128 &&
    name.split(".").every((part) => part.length > 0 && /^[a-z0-9_-]+$/.test(part))
  );
}
