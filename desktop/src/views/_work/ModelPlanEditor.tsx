/**
 * An agent has a **plan**, not a model.
 *
 * A `ModelPlan` is an ordered list plus a strategy for choosing among it, so a
 * quota wall on the first model is a retry rather than a dead agent. Three
 * things make that legible rather than magical, and this editor owes all
 * three:
 *
 * 1. **The strategies are not self-evident.** "Weighted" and "least busy"
 *    describe an implementation, not an outcome, so each one carries a line
 *    saying what it actually does to this list.
 * 2. **Order is part of the plan for every strategy**, not just fallback —
 *    it is the fall-through chain, the round-robin ring and the tie-breaker.
 *    So the arrows are always live, and the hint says what order means here.
 * 3. **Health is live.** `GET /models/health` is the engine's in-process
 *    ledger: which `(harness, model)` pairs are cooling down, how long is
 *    left, how many failures in a row, how many sessions are in flight. It is
 *    the answer to "why is my agent on its second-choice model?", which is
 *    the whole point of the failover work.
 *
 * The model list comes from `GET /harnesses/{id}/models`, and **free text
 * stays available regardless**: an empty list means the harness did not tell
 * us, which is *unknown*, never *none*.
 *
 * The plan also says **how hard its models work** (06 §Effort): an effort of
 * its own — *Inherit* leaves it to the `agents.effort` setting, *Auto* to
 * the Decision-Making Agent — and one per model for a model that should
 * differ. Each picker offers only what its model takes on this harness, and
 * the line under it says what runs and who decided. A level a model cannot
 * take is fitted at launch, so a saved value is never dropped here: it stays
 * in its picker, marked. The rules are `effortModel.mjs`'s; the plan's own —
 * the strategies, the edits, what the ledger says of a model — are
 * `modelPlanModel.mjs`'s.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import type { Effort, ModelHealthRow, ModelInfo, ModelPlan, ModelStrategy } from "../../types";
import { Button, Chip, Field, ICON, Labelled, Select, TextInput, Tooltip } from "../../ui";
import { useAsync } from "./useAsync";
import { useVisible } from "../../shell/visibility";
import { EffortPicker, useEffortSetting } from "./EffortPicker";
import { effortOptions, effortWords, effortsFor, offersEffort, planEfforts, resolveEffort, withEffort } from "./effortModel.mjs";
import { STRATEGIES, addModel, answeredBy, enabledCount, healthChips, healthOf, moveModel, patchModel, removeModel, strategyOf, unusedModels, weightFrom } from "./modelPlanModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** How often the health ledger is re-read while this editor is open. */
const HEALTH_POLL_MS = 15_000;

/**
 * The engine's model-health ledger, kept fresh while a screen is watching.
 *
 * In-process and per-node: a restart forgets every cooldown, so an empty
 * answer means "nothing to report", never "no models".
 */
export function useModelHealth(): {
  rows: ModelHealthRow[];
  error: string | null;
  reload: () => void;
} {
  const [tick, setTick] = useState(0);
  const { data, error, reload } = useAsync((s) => api.modelHealth(s), [tick]);
  // No health polling while the window is hidden.
  const visible = useVisible();
  useEffect(() => {
    if (!visible) return;
    const t = window.setInterval(() => setTick((n) => n + 1), HEALTH_POLL_MS);
    return () => window.clearInterval(t);
  }, [visible]);
  return { rows: data?.models ?? [], error, reload };
}

/**
 * What the engine currently knows about one `(harness, model)` pair.
 *
 * Silent when there is nothing to say — a healthy idle model earns no badge,
 * so the ones that do carry a badge are worth reading. Which badges, in what
 * tone and words, is `modelPlanModel.healthChips`.
 */
export function ModelHealthBadges({ row }: { row: ModelHealthRow | undefined }) {
  return (
    <>
      {healthChips(row).map((chip) => (
        <Tooltip key={chip.id} label={chip.tip}>
          <span>
            <Chip tone={chip.tone} icon={ICON[chip.icon]}>
              {chip.words}
            </Chip>
          </span>
        </Tooltip>
      ))}
    </>
  );
}

export function ModelPlanEditor({
  harness,
  plan,
  onChange,
  disabled,
}: {
  harness: string;
  plan: ModelPlan;
  onChange: (next: ModelPlan) => void;
  disabled?: boolean;
}) {
  const [pending, setPending] = useState("");
  const [addError, setAddError] = useState<string | null>(null);

  // What the harness says it can run. A failure is not an empty list: one
  // means "it didn't tell us", the other means "it told us nothing".
  const {
    data: modelData,
    error: modelsError,
    loading: modelsLoading,
  } = useAsync(
    async (s) => (harness ? await api.models(harness, s) : { harness: "", efforts: [] as Effort[], models: [] as ModelInfo[] }),
    [harness],
  );
  const offered = modelData?.models ?? [];

  // What this harness takes, once it has answered for itself: an answer kept
  // from the harness picked before is not this one's, and until this one
  // answers its levels are unknown, not none.
  const answered = answeredBy(harness, modelData, modelsError);
  const known = answered !== null;
  const { setting, origin } = useEffortSetting(null);
  const planTaken = planEfforts(answered, plan);
  const planWords = effortWords(resolveEffort(null, null, plan.effort, setting), { available: planTaken, known, origin });

  const { rows: health, error: healthError } = useModelHealth();
  const models = plan.models ?? [];
  const def = strategyOf(plan);
  const strategy: ModelStrategy = def.value;

  const add = (id: string) => {
    const added = addModel(plan, id);
    setAddError(added.error);
    if (!added.plan) return;
    onChange(added.plan);
    setPending("");
  };

  const unused = unusedModels(offered, plan);

  return (
    <div className="flex flex-col gap-3">
      <Field label={tr("work-model-plan-editor-strategy")} hint={def.explain}>
        <Select
          value={strategy}
          disabled={disabled}
          onChange={(e) => onChange({ ...plan, strategy: strategyOf({ strategy: e.target.value }).value })}
        >
          {STRATEGIES.map((s) => (
            <option key={s.value} value={s.value}>
              {s.label}
            </option>
          ))}
        </Select>
      </Field>

      <Field label={tr("work-model-plan-editor-effort")} hint={planWords.hint}>
        <EffortPicker
          value={plan.effort}
          options={effortOptions(planTaken, { inherit: true, auto: true, known, current: plan.effort })}
          disabled={disabled}
          onChange={(effort) => onChange(withEffort(plan, effort))}
        />
      </Field>

      <Labelled
        label={tr("work-model-plan-editor-models-enabled", { enabledCount: enabledCount(plan), models: models.length })}
        hint={
          models.length === 0
            ? tr("work-model-plan-editor-empty-plan-honest-way-say-whatever")
            : def.orderMeans
        }
      >
        <div className="flex flex-col gap-1.5">
          {models.length === 0 && (
            <div className="flex flex-wrap items-center gap-1.5 rounded-control bg-surface-2/50 px-2 py-1.5">
              <span className="font-mono text-2xs text-text-dim">{tr("work-model-plan-editor-default", { harness })}</span>
              {/* The ledger's own name for an unpinned session on this harness. */}
              <ModelHealthBadges row={healthOf(health, harness, null)} />
            </div>
          )}

          {models.map((m, i) => {
            const on = m.enabled !== false;
            // This model's own levels, and what it runs at: its own effort, else the plan's, else the setting.
            const taken = effortsFor(answered, m.model);
            const words = effortWords(resolveEffort(null, m.effort, plan.effort, setting), { available: taken, known, origin });
            const choices = effortOptions(taken, { inherit: true, auto: true, known, current: m.effort });
            // A row says its effort when it has one to pick, or when it differs from what the plan just said.
            const saysEffort = offersEffort(choices) || words.hint !== planWords.hint;
            return (
              <div
                key={m.model}
                className="flex flex-col gap-1.5 rounded-control bg-surface-2/50 px-2 py-1.5"
              >
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="tnum w-5 shrink-0 text-right text-2xs text-text-dim">{i + 1}.</span>
                  <Tooltip label={m.model}>
                    <span
                      className={`min-w-0 flex-1 truncate font-mono text-xs ${
                        on ? "" : "text-text-dim line-through"
                      }`}
                    >
                      {m.model}
                    </span>
                  </Tooltip>
                  <ModelHealthBadges row={healthOf(health, harness, m.model)} />
                  {strategy === "weighted" && (
                    <label className="flex shrink-0 items-center gap-1 text-2xs text-text-dim">{tr("work-model-plan-editor-weight")}<TextInput
                        className="w-14"
                        inputMode="numeric"
                        disabled={disabled}
                        value={String(m.weight ?? 1)}
                        aria-label={tr("work-model-plan-editor-weight-2", { model: m.model })}
                        onChange={(e) => onChange(patchModel(plan, i, { weight: weightFrom(e.target.value) }))}
                      />
                    </label>
                  )}
                  <EffortPicker
                    className="w-auto"
                    value={m.effort}
                    options={choices}
                    disabled={disabled}
                    caption={tr("work-model-plan-editor-effort-row")}
                    label={tr("work-model-plan-editor-effort-for", { model: m.model })}
                    onChange={(effort) => onChange({ ...plan, models: models.map((c, j) => (j === i ? withEffort(c, effort) : c)) })}
                  />
                  {/* The kit's own reorder glyphs (`ICON.up` / `ICON.down`,
                      the rebase editor's), not the navigation arrows: those
                      would say "back" where the control means "one position
                      earlier in the plan". Both carry an aria-label, which is
                      what a screen reader reads either way. */}
                  <div className="flex shrink-0 items-center">
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={tr("work-model-plan-editor-move-up", { model: m.model })}
                      disabled={disabled || i === 0}
                      onClick={() => onChange(moveModel(plan, i, -1))}
                    >
                      <ICON.up size={12} aria-hidden />
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={tr("work-model-plan-editor-move-down", { model: m.model })}
                      disabled={disabled || i === models.length - 1}
                      onClick={() => onChange(moveModel(plan, i, 1))}
                    >
                      <ICON.down size={12} aria-hidden />
                    </Button>
                    <Tooltip label={tr("work-model-plan-editor-off-without-losing-place-order")}>
                      <span className="inline-flex">
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={disabled}
                          onClick={() => onChange(patchModel(plan, i, { enabled: !on }))}
                        >
                          {on ? tr("work-model-plan-editor-disable") : tr("work-model-plan-editor-enable")}
                        </Button>
                      </span>
                    </Tooltip>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={disabled}
                      onClick={() => onChange(removeModel(plan, i))}
                    >
                      <ICON.delete size={12} aria-hidden />{tr("work-model-plan-editor-remove")}</Button>
                  </div>
                </div>
                {strategy === "auto_route" && (
                  <label className="flex items-center gap-1.5 text-2xs text-text-dim">{tr("work-model-plan-editor-best")}<TextInput
                      className="min-w-0 flex-1"
                      disabled={disabled}
                      value={m.suited_for ?? ""}
                      placeholder={tr("work-model-plan-editor-design-work-needs-second-opinion")}
                      aria-label={tr("work-model-plan-editor-what-best", { model: m.model })}
                      onChange={(e) => onChange(patchModel(plan, i, { suited_for: e.target.value || null }))}
                    />
                  </label>
                )}
                {saysEffort && <p className="text-2xs text-text-dim">{words.hint}</p>}
              </div>
            );
          })}

          <div className="flex flex-wrap items-center gap-2">
            {unused.length > 0 && (
              <Select
                className="w-auto min-w-40"
                value=""
                disabled={disabled}
                aria-label={tr("work-model-plan-editor-add-model-harness-offers")}
                onChange={(e) => e.target.value && add(e.target.value)}
              >
                <option value="">{tr("work-model-plan-editor-add-one-offers")}</option>
                {unused.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label ?? m.id}
                  </option>
                ))}
              </Select>
            )}
            <TextInput
              className="w-auto min-w-40 flex-1 font-mono"
              value={pending}
              disabled={disabled}
              placeholder={tr("work-model-plan-editor-type-model-id")}
              aria-label={tr("work-model-plan-editor-add-model-id")}
              onChange={(e) => {
                setPending(e.target.value);
                setAddError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  add(pending);
                }
              }}
            />
            <Button disabled={disabled || !pending.trim()} onClick={() => add(pending)}>{tr("work-model-plan-editor-add")}</Button>
          </div>

          {addError && <p className="text-2xs text-danger">{addError}</p>}

          <p className="text-2xs text-text-dim">
            {modelsLoading
              ? tr("work-model-plan-editor-asking-harness-what-can-run")
              : modelsError
                ? tr("work-model-plan-editor-couldn-t-ask-models-type-ids", { harness, modelsError })
                : offered.length === 0
                  ? tr("work-model-plan-editor-doesn-t-list-models-unknown-not", { harness })
                  : tr("work-model-plan-editor-offers-model-models-anything-else-accepts", { harness, offered: offered.length })}
          </p>
        </div>
      </Labelled>

      <div className="rounded-control bg-surface-2/50 px-2 py-1.5">
        <p className="text-2xs font-semibold text-text-dim">{tr("work-model-plan-editor-live-health")}</p>
        <p className="mt-0.5 max-w-measure text-2xs leading-relaxed text-text-dim">
          {healthError
            ? tr("work-model-plan-editor-couldn-t-read-ledger", { healthError })
            : health.length === 0
              ? tr("work-model-plan-editor-nothing-report-ledger-process-restart-forgets")
              : tr("work-model-plan-editor-cooling-model-not-dropped-from-plan")}
        </p>
      </div>
    </div>
  );
}
