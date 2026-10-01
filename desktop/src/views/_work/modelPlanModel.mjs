/**
 * An agent's model plan as the editor and the agent's card read it (06
 * §Model plans and failover): the strategies and what each does to the
 * list, the edits a person makes — a model added once, moved, weighted,
 * switched off, removed — what the engine's health ledger says of one
 * `(harness, model)` pair, and whether the harness that answered is the one
 * asked. The effort a plan names is `effortModel.mjs`'s.
 *
 * Every edit returns a new plan and leaves the one it was handed alone.
 * Pure, so `node --test` reads it; the test beside it holds the strategies
 * and the ledger's key equal to the Rust.
 */

import { durationPrecise } from "../../i18n/format.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The strategy a plan has when it names none — the core's `ModelStrategy::default`. */
export const DEFAULT_STRATEGY = "fallback";

/**
 * The strategies, in the core's order: its word on the wire, its label, what
 * it does to *this* list in one line, and what the order of the list means
 * under it — order is part of the plan for every one of them.
 */
export const STRATEGIES = Object.freeze([
  {
    value: "fallback",
    label: t("work-model-plan-editor-fallback"),
    explain: t("work-model-plan-editor-takes-models-strictly-top-down-uses"),
    orderMeans: t("work-model-plan-editor-order-plan-top-model-runs-rest"),
  },
  {
    value: "weighted",
    label: t("work-model-plan-editor-weighted"),
    explain: t("work-model-plan-editor-picks-model-leads-proportion-weight-weight"),
    orderMeans: t("work-model-plan-editor-order-breaks-ties-between-equal-weights"),
  },
  {
    value: "round_robin",
    label: t("work-model-plan-editor-round-robin"),
    explain: t("work-model-plan-editor-starts-each-launch-one-position-further"),
    orderMeans: t("work-model-plan-editor-order-ring-rotation-walks"),
  },
  {
    value: "least_busy",
    label: t("work-model-plan-editor-least-busy"),
    explain: t("work-model-plan-editor-leads-whichever-model-has-fewest-sessions"),
    orderMeans: t("work-model-plan-editor-order-breaks-ties-when-two-models"),
  },
  {
    value: "auto_route",
    label: t("work-model-plan-editor-auto-route"),
    explain: t("work-model-plan-editor-decision-making-agent-picks-model-suits-each"),
    orderMeans: t("work-model-plan-editor-order-breaks-ties-fallback-while-decision"),
  },
]);

/**
 * A plan's strategy with its words; a plan that names none, or a word this
 * build does not know, reads as the default.
 * @param {{strategy?: unknown} | null | undefined} plan
 */
export function strategyOf(plan) {
  return STRATEGIES.find((s) => s.value === plan?.strategy) ?? STRATEGIES[0];
}

/**
 * The name the engine's ledger files an unpinned session under — a plan
 * that names no model runs the harness's own (`models.rs::model_key`). A key
 * another program wrote, never a word for a person: the label a row shows
 * is the catalog's.
 * @param {string} harness
 */
export function defaultModelKey(harness) {
  return `${harness} default`; // for the machine
}

/**
 * What the ledger knows of one model on one harness — never of the model
 * alone: a cooldown is true of this harness's account. `null` for the model
 * asks about the harness's own.
 * @param {readonly {harness: string, model: string}[] | null | undefined} rows
 * @param {string} harness
 * @param {string | null} model
 */
export function healthOf(rows, harness, model) {
  const key = model === null ? defaultModelKey(harness) : model;
  return (rows ?? []).find((r) => r.harness === harness && r.model === key);
}

/**
 * What a row wears for what the ledger knows, in order: cooling with how
 * long is left, the failures in a row, the sessions in flight. Nothing for a
 * healthy idle model — so a badge that shows is worth reading. The failures
 * are quiet beside a cooldown, which already says why.
 * @param {{retry_in_secs?: number, consecutive_failures?: number, in_flight?: number} | null | undefined} row
 * @returns {{id: "cooling" | "failures" | "flight", tone: "warn" | "quiet" | "accent", icon: "waiting" | "warn" | "working", words: string, tip: string}[]}
 */
export function healthChips(row) {
  if (!row) return [];
  const left = Number(row.retry_in_secs);
  const cooling = Number.isFinite(left) && left > 0;
  const failures = Number.isInteger(row.consecutive_failures) ? row.consecutive_failures : 0;
  const flying = Number.isInteger(row.in_flight) ? row.in_flight : 0;
  const chips = [];
  if (cooling) chips.push({ id: "cooling", tone: "warn", icon: "waiting", words: t("work-model-plan-editor-cooling-retry", { time: durationPrecise(Math.ceil(left)) }), tip: t("work-model-plan-editor-model-told-harness-come-back-later") });
  if (failures > 0) chips.push({ id: "failures", tone: cooling ? "quiet" : "warn", icon: "warn", words: t("work-model-plan-editor-failures", { n: failures }), tip: t("work-model-plan-editor-failures-row-since-last-success") });
  if (flying > 0) chips.push({ id: "flight", tone: "accent", icon: "working", words: t("work-model-plan-editor-flight", { in_flight: flying }), tip: t("work-model-plan-editor-sessions-running-model-right-now") });
  return chips;
}

/** A model's share under `weighted`, as a row says it. @param {unknown} weight */
export function weightWords(weight) {
  return t("work-model-plan-editor-weighs", { weight: weightFrom(weight) });
}

/**
 * A weight as it is typed: a whole number from one — the core clamps a zero
 * to one, so an enabled model is never out of reach; *Disable* is how one is
 * switched off. Anything that is no number is one.
 * @param {unknown} typed
 */
export function weightFrom(typed) {
  const n = Math.trunc(Number(typed));
  return Number.isFinite(n) && n >= 1 ? Math.min(n, MAX_WEIGHT) : 1;
}

/** The most a weight may be — the wire's `u32`. */
export const MAX_WEIGHT = 4294967295;

/**
 * What a harness answered when asked for its models, once it is **this**
 * harness that answered: an answer kept from the harness picked before is
 * not this one's, and a read that failed is no answer. `null` means unknown
 * — never none.
 * @template {{harness?: string}} A
 * @param {string} harness the harness asked
 * @param {A | null | undefined} answer
 * @param {unknown} [error]
 * @returns {A | null}
 */
export function answeredBy(harness, answer, error = null) {
  if (!harness || error || !answer) return null;
  return answer.harness === harness ? answer : null;
}

const modelsOf = (plan) => plan?.models ?? [];

/**
 * A model added at the end of the plan, enabled, weighing one — or why not:
 * an id already in the plan is said, never added twice, since the ledger and
 * the walk both know a model by its id. A blank id is nothing to add.
 * @param {{models?: readonly {model: string}[]}} plan
 * @param {string} id
 * @returns {{plan: object, error: null} | {plan: null, error: string | null}}
 */
export function addModel(plan, id) {
  const model = String(id ?? "").trim();
  if (model === "") return { plan: null, error: null };
  if (modelsOf(plan).some((m) => m.model === model)) return { plan: null, error: t("work-model-plan-editor-already-plan", { model }) };
  return { plan: { ...plan, models: [...modelsOf(plan), { model, weight: 1, enabled: true }] }, error: null };
}

/**
 * A model moved one place up (`-1`) or down (`+1`); the same plan when it
 * is already at that end, or is not there.
 * @template {{models?: readonly object[]}} P
 * @param {P} plan @param {number} from @param {number} delta
 * @returns {P}
 */
export function moveModel(plan, from, delta) {
  const models = [...modelsOf(plan)];
  const to = from + delta;
  if (!Number.isInteger(from) || !Number.isInteger(to) || from < 0 || to < 0 || from >= models.length || to >= models.length || from === to) return plan;
  [models[from], models[to]] = [models[to], models[from]];
  return { ...plan, models };
}

/**
 * One model of the plan with some of its fields set; the same plan when it
 * is not there.
 * @template {{models?: readonly object[]}} P
 * @param {P} plan @param {number} at @param {object} patch
 * @returns {P}
 */
export function patchModel(plan, at, patch) {
  const models = modelsOf(plan);
  if (!Number.isInteger(at) || at < 0 || at >= models.length) return plan;
  return { ...plan, models: models.map((m, i) => (i === at ? { ...m, ...patch } : m)) };
}

/**
 * The plan without one model; the same plan when it is not there.
 * @template {{models?: readonly object[]}} P
 * @param {P} plan @param {number} at
 * @returns {P}
 */
export function removeModel(plan, at) {
  const models = modelsOf(plan);
  if (!Number.isInteger(at) || at < 0 || at >= models.length) return plan;
  return { ...plan, models: models.filter((_, i) => i !== at) };
}

/** The models a harness offers that the plan does not hold yet. @param {readonly {id: string}[] | null | undefined} offered @param {{models?: readonly {model: string}[]} | null | undefined} plan */
export function unusedModels(offered, plan) {
  const held = new Set(modelsOf(plan).map((m) => m.model));
  return (offered ?? []).filter((m) => !held.has(m.id));
}

/** How many of a plan's models may run: the ones not switched off. @param {{models?: readonly {enabled?: boolean}[]} | null | undefined} plan */
export function enabledCount(plan) {
  return modelsOf(plan).filter((m) => m.enabled !== false).length;
}
