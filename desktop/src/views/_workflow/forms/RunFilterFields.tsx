/**
 * A run filter's fields — what the run does (queued, started, ended, a
 * step's move), runs of which workflows, ending how, which steps and
 * states, which goals' runs — shared by a start that begins when a run
 * moves and a wait that holds for one. Each takes one or several, any of
 * which is heard; a run that ends is what a run event always meant, and is
 * what it means when nothing else is said.
 */

import type { RunEnd } from "../../../types";
import { api } from "../../../api";
import { ICON, Labelled } from "../../../ui";
import { useAsync } from "../../_work/useAsync";
import { RUN_CHANGES, RUN_ENDS } from "../stepKinds.mjs";
import { WorkflowPicker } from "../WorkflowPicker";
import type { OneOrMany, RunChange, RunFilterEx } from "./eventFilters";
import { WordChecks, WordsInput } from "./ManyFields";
import { addWord, removeWord } from "./manyModel.mjs";
import { asList } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

/**
 * Workflows, one or several: each a chip by its name — by its id when the
 * library no longer has it — and the picker to add the next; none is any.
 */
export function WorkflowRefsInput({
  value,
  onChange,
  disabled,
  addLabel,
}: {
  value: OneOrMany<string> | null | undefined;
  onChange: (next: OneOrMany<string> | null) => void;
  disabled?: boolean;
  addLabel: string;
}) {
  const library = useAsync((s) => api.workflows({ scope: "library" }, s), []);
  const names = new Map((library.data?.workflows ?? []).map((row) => [row.workflow.id, row.workflow.name]));
  const ids = asList(value);
  return (
    <div className="flex flex-col gap-1.5">
      {ids.length > 0 && (
        <div className="flex flex-wrap items-center gap-1">
          {ids.map((id) => {
            const word = names.get(id) ?? id;
            return (
              <button
                key={id}
                type="button"
                title={disabled ? undefined : t("workflow-many-fields-remove", { word })}
                aria-label={disabled ? word : t("workflow-many-fields-remove", { word })}
                disabled={disabled}
                onClick={() => onChange(removeWord(value, id))}
                className="anim inline-flex h-5 max-w-full items-center gap-1 rounded-full bg-surface-2 pr-1.5 pl-2 text-2xs text-text hover:bg-selected disabled:opacity-60"
              >
                <span className="truncate">{word}</span>
                {!disabled && <ICON.close size={10} aria-hidden className="shrink-0 text-text-dim" />}
              </button>
            );
          })}
        </div>
      )}
      <WorkflowPicker value={null} disabled={disabled} allowNone noneLabel={ids.length ? t("workflow-many-fields-add") : addLabel} ariaLabel={addLabel} onChange={(id) => id && onChange(addWord(value, id))} />
    </div>
  );
}

const CHANGES = RUN_CHANGES.map((c) => ({ word: c.change as RunChange, label: c.label }));
const ENDS = RUN_ENDS.map((r) => ({ word: r.outcome as RunEnd, label: r.label }));

export function RunFilterFields<F extends object>({ value, onChange, disabled }: { value: F; onChange: (next: F) => void; disabled?: boolean }) {
  const filter = value as RunFilterEx;
  const set = (patch: Partial<RunFilterEx>) => onChange({ ...value, ...patch });
  const changes = asList(filter.change ?? "ended");
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-run-filter-fields-change")} hint={t("workflow-run-filter-fields-change-hint")}>
        <WordChecks options={CHANGES} value={filter.change} fallback="ended" columns={2} disabled={disabled} onChange={(change) => set({ change })} />
      </Labelled>
      <Labelled label={t("workflow-run-filter-fields-workflows")} hint={t("workflow-run-filter-fields-workflows-hint")}>
        <WorkflowRefsInput value={filter.workflow} disabled={disabled} addLabel={t("workflow-run-filter-fields-add-workflow")} onChange={(workflow) => set({ workflow })} />
      </Labelled>
      {changes.includes("ended") && (
        <Labelled label={t("workflow-run-filter-fields-ending")} hint={t("workflow-run-filter-fields-ending-hint")}>
          <WordChecks options={ENDS} value={filter.outcome} fallback={null} columns={3} disabled={disabled} onChange={(outcome) => set({ outcome })} />
        </Labelled>
      )}
      {changes.includes("step") && (
        <div className="grid gap-3 @sm:grid-cols-2">
          <Labelled label={t("workflow-run-filter-fields-steps")} hint={t("workflow-run-filter-fields-steps-hint")}>
            <WordsInput mono value={filter.step} disabled={disabled} /* for the machine */ placeholder="draft" aria-label={t("workflow-run-filter-fields-steps")} onChange={(step) => set({ step })} />
          </Labelled>
          <Labelled label={t("workflow-run-filter-fields-states")} hint={t("workflow-run-filter-fields-states-hint")}>
            <WordsInput mono value={filter.state} disabled={disabled} /* for the machine */ placeholder="failed" aria-label={t("workflow-run-filter-fields-states")} onChange={(state) => set({ state })} />
          </Labelled>
        </div>
      )}
      <Labelled label={t("workflow-run-filter-fields-goal")} hint={t("workflow-run-filter-fields-goal-hint")}>
        <WordsInput mono value={filter.goal} disabled={disabled} /* for the machine */ placeholder="{inputs.goal}" aria-label={t("workflow-run-filter-fields-goal")} onChange={(goal) => set({ goal })} />
      </Labelled>
    </div>
  );
}
