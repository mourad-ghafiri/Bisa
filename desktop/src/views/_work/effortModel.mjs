/**
 * Effort: how hard a model works on a task (06 §Effort) — the desktop's
 * mirror of `crates/bisa-core/src/effort.rs`, and the facts every effort
 * control draws from: the six levels and `auto`, what a harness takes for a
 * model, what a picker offers, who decides, and the one sentence that says
 * what runs and why.
 *
 * Two words are kept apart, as the core keeps them. An **effort** is a level
 * a model runs at. A **choice** is what somebody asks for: a level, or
 * `auto` — the Decision-Making Agent reads the task and names the level.
 * Nothing chosen is absent (`null`), never `""`: it inherits.
 *
 * A level a model cannot take is fitted at launch, never refused, so a value
 * already saved stays in its picker when the model stops offering it — shown,
 * and marked. Pure, so `node --test` reads it; the test beside it holds the
 * words equal to the Rust.
 */

import { t } from "../../i18n/l10n.mjs";

/** The levels, lowest first — the core's `Effort::ALL`. */
export const EFFORTS = Object.freeze(["minimal", "low", "medium", "high", "xhigh", "max"]);

const AUTO = "auto";

/** What may be asked for — the core's `EffortChoice::ALL`. */
export const EFFORT_CHOICES = Object.freeze([AUTO, ...EFFORTS]);

/** What runs when nobody says otherwise — the core's `Effort::DEFAULT`. */
export const DEFAULT_EFFORT = "high";

/** The setting at the end of the chain: a project's, then the workspace's. */
export const EFFORT_SETTING = "agents.effort";

/** A choice's label. The wire's word is data; this is what a person reads. */
const LABEL = Object.freeze({
  auto: t("work-effort-auto"),
  minimal: t("work-effort-minimal"),
  low: t("work-effort-low"),
  medium: t("work-effort-medium"),
  high: t("work-effort-high"),
  xhigh: t("work-effort-xhigh"),
  max: t("work-effort-max"),
});

/** The picker's word for nothing chosen. */
const INHERIT = t("work-effort-inherit");

/**
 * A value read as a choice, or `null` when it is none — absent, empty, or a
 * word this build does not know.
 * @param {unknown} value
 */
export function effortChoice(value) {
  return typeof value === "string" && EFFORT_CHOICES.includes(value) ? value : null;
}

/**
 * A choice's label: *Inherit* for nothing chosen, the word itself for one
 * this build does not know.
 * @param {unknown} value
 */
export function effortLabel(value) {
  if (value === null || value === undefined || value === "") return INHERIT;
  const choice = effortChoice(value);
  return choice === null ? String(value) : LABEL[choice];
}

/** The levels of a list, each once, lowest first; a word that is no level is left out. */
function ordered(levels) {
  const listed = Array.isArray(levels) ? levels : [];
  return EFFORTS.filter((level) => listed.includes(level));
}

/**
 * The level to run at when only `available` can be taken: this one, else
 * the nearest below it, else the lowest above it; `null` when nothing is
 * available. The core's `Effort::clamp_to`.
 * @param {string} level
 * @param {readonly string[]} available
 */
export function clampEffort(level, available) {
  const rank = EFFORTS.indexOf(level);
  if (rank < 0) return null;
  const taken = ordered(available);
  const below = taken.filter((e) => EFFORTS.indexOf(e) <= rank);
  return below[below.length - 1] ?? taken[0] ?? null;
}

/**
 * What a harness takes for one model, from `GET /harnesses/{id}/models`: the
 * listed model's own levels; for an id the harness does not list — typed by
 * hand, or none at all — the levels it takes for any model. Nothing when the
 * harness has not answered.
 * @param {{efforts?: readonly string[], models?: readonly {id: string, efforts?: readonly string[]}[]} | null | undefined} harnessModels
 * @param {string | null | undefined} modelId
 */
export function effortsFor(harnessModels, modelId) {
  if (!harnessModels) return [];
  const listed = modelId ? (harnessModels.models ?? []).find((m) => m.id === modelId) : undefined;
  return ordered(listed ? listed.efforts : harnessModels.efforts);
}

/**
 * What an agent's plan can run at on its harness: every level one of its
 * enabled models takes. A level is fitted to each model at launch, so the
 * plan's picker offers what any of them takes. An empty plan runs the
 * harness's own model.
 * @param {Parameters<typeof effortsFor>[0]} harnessModels
 * @param {{models?: readonly {model: string, enabled?: boolean}[]} | null | undefined} plan
 */
export function planEfforts(harnessModels, plan) {
  const models = (plan?.models ?? []).filter((m) => m.enabled !== false);
  if (models.length === 0) return effortsFor(harnessModels, null);
  return ordered(models.flatMap((m) => effortsFor(harnessModels, m.model)));
}

/**
 * What several harnesses take for one model, together — a step names its
 * harnesses as a fallback order, and any of them may run it.
 * @param {readonly Parameters<typeof effortsFor>[0][]} answers one per harness; `null` for one that did not answer
 * @param {string | null | undefined} modelId
 */
export function effortsAcross(answers, modelId) {
  return ordered((answers ?? []).flatMap((a) => effortsFor(a, modelId)));
}

/**
 * What a picker offers. *Inherit* (nothing chosen) and *Auto* when asked
 * for, then only the levels in `available`. With nothing available and a
 * harness that has not answered (`known: false`), all six: unknown is not
 * none. With nothing available from a harness that has answered, no level
 * is offered — `effortWords` says so. *Auto* needs two levels to choose
 * between.
 *
 * `current` — the value already saved — is always among the options: one no
 * longer offered is kept last, marked with the level it is fitted to.
 * @param {readonly string[]} available
 * @param {{inherit?: boolean, auto?: boolean, known?: boolean, current?: unknown}} [asked]
 * @returns {{value: string, label: string, kept?: boolean}[]}
 */
export function effortOptions(available, { inherit = false, auto = false, known = true, current = null } = {}) {
  const taken = ordered(available);
  const levels = taken.length === 0 && !known ? [...EFFORTS] : taken;
  const options = [];
  if (inherit) options.push({ value: "", label: effortLabel("") });
  if (auto && levels.length >= 2) options.push({ value: AUTO, label: effortLabel(AUTO) });
  for (const level of levels) options.push({ value: level, label: effortLabel(level) });
  const saved = effortChoice(current);
  if (saved !== null && !options.some((o) => o.value === saved)) {
    const fitted = clampEffort(saved, levels);
    options.push({
      value: saved,
      label: fitted === null ? t("work-effort-kept", { label: LABEL[saved] }) : t("work-effort-kept-fitted", { label: LABEL[saved], level: LABEL[fitted] }),
      kept: true,
    });
  }
  return options;
}

/**
 * Whether a picker has anything to pick: a level, *Auto*, or a saved value
 * to see and clear. *Inherit* alone is no choice.
 * @param {ReturnType<typeof effortOptions>} options
 */
export function offersEffort(options) {
  return (options ?? []).some((o) => o.value !== "");
}

/**
 * Who decides, first that is set: the step's pin, the model's own, the
 * agent's plan, the setting. `auto` falls back to the next level further
 * down the chain that is a level, and to the default when there is none. The
 * core's `resolve`, with where each answer came from — what the hint says.
 * @param {unknown} step
 * @param {unknown} model
 * @param {unknown} plan
 * @param {unknown} setting
 * @returns {{kind: "level", level: string, from: string} | {kind: "auto", fallback: string, from: string, fallbackFrom: string}}
 */
export function resolveEffort(step, model, plan, setting) {
  const chain = [
    { from: "step", choice: effortChoice(step) },
    { from: "model", choice: effortChoice(model) },
    { from: "plan", choice: effortChoice(plan) },
    { from: "setting", choice: effortChoice(setting) ?? DEFAULT_EFFORT },
  ].filter((link) => link.choice !== null);
  const [first, ...further] = chain;
  if (first.choice !== AUTO) return { kind: "level", level: first.choice, from: first.from };
  const next = further.find((link) => link.choice !== AUTO);
  return { kind: "auto", fallback: next?.choice ?? DEFAULT_EFFORT, from: first.from, fallbackFrom: next?.from ?? "default" };
}

/**
 * The `agents.effort` setting as resolved for the workspace or a project:
 * the choice, and the layer that holds it. A value that is no choice reads
 * as the default.
 * @param {readonly {key: string, value: unknown, origin?: string}[] | null | undefined} resolved
 * @returns {{setting: string, origin: string}}
 */
export function effortSetting(resolved) {
  const row = (resolved ?? []).find((r) => r.key === EFFORT_SETTING);
  const setting = effortChoice(row?.value);
  return setting === null ? { setting: DEFAULT_EFFORT, origin: "default" } : { setting, origin: row?.origin ?? "default" };
}

/**
 * What one attempt comes to, in words: the level that runs once fitted to
 * what the model takes, a short word for a row, and the sentence under a
 * control — *Runs at High — the workspace's setting.* Where the harness
 * answered with no level there is none to run at, and the sentence says so.
 * The setting's link is named by the layer that holds it (`origin`).
 * @param {ReturnType<typeof resolveEffort>} resolved
 * @param {{available?: readonly string[], known?: boolean, origin?: string}} [facts]
 * @returns {{runs: string | null, label: string, hint: string}}
 */
export function effortWords(resolved, { available = [], known = false, origin = "default" } = {}) {
  const taken = ordered(available);
  if (known && taken.length === 0) return { runs: null, label: "", hint: t("work-effort-no-levels") };
  const fit = (level) => (taken.length === 0 ? level : (clampEffort(level, taken) ?? level));
  if (resolved.kind === "auto") {
    const runs = fit(resolved.fallback);
    // Fewer than two levels is no question: the fallback runs, and nobody is asked.
    const hint = taken.length === 1 ? t("work-effort-hint-auto-one", { level: LABEL[runs] }) : t("work-effort-hint-auto", { level: LABEL[runs] });
    return { runs, label: taken.length === 1 ? LABEL[runs] : LABEL.auto, hint };
  }
  const runs = fit(resolved.level);
  const from = resolved.from === "setting" ? origin : resolved.from;
  const hint = runs === resolved.level ? t("work-effort-hint-runs", { level: LABEL[runs], from }) : t("work-effort-hint-fitted", { level: LABEL[runs], asked: LABEL[resolved.level], from });
  return { runs, label: LABEL[runs], hint };
}

/**
 * A plan, a model of it or a step with its effort set — or without the
 * field when nothing is chosen, which is how it inherits. A new object.
 * @template {object} T
 * @param {T} holder
 * @param {unknown} effort
 * @returns {T}
 */
export function withEffort(holder, effort) {
  const next = { ...holder };
  delete next.effort;
  const choice = effortChoice(effort);
  return choice === null ? next : { ...next, effort: choice };
}
