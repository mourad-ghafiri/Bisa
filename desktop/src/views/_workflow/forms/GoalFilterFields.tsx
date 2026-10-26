/**
 * A goal filter's fields — which goal's move, and which moves — shared by a
 * start that begins when a goal moves and a wait that holds for one. A goal
 * is named by its id, or by a template over the inputs (a parent naming the
 * child it spawned); one, several, or none for any goal's. It is named by
 * its id's shape, never by whether it is still here: a start on a goal's
 * `closed` or `deleted` fires when the goal goes.
 */

import { Labelled } from "../../../ui";
import { GOAL_CHANGES } from "../stepKinds.mjs";
import type { GoalChange, GoalFilterEx } from "./eventFilters";
import { WordChecks, WordsInput } from "./ManyFields";
import { t } from "../../../i18n/l10n.mjs";

const CHANGES = GOAL_CHANGES.map((c) => ({ word: c.change as GoalChange, label: c.label }));

export function GoalFilterFields<F extends object>({ value, onChange, disabled, templateHint }: { value: F; onChange: (next: F) => void; disabled?: boolean; templateHint: string }) {
  const filter = value as GoalFilterEx;
  const set = (patch: Partial<GoalFilterEx>) => onChange({ ...value, ...patch });
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-goal-filter-fields-goal")} hint={t("workflow-goal-filter-fields-goal-hint", { hint: templateHint })}>
        <WordsInput mono value={filter.goal} disabled={disabled} /* for the machine */ placeholder="{inputs.goal}" aria-label={t("workflow-goal-filter-fields-goal")} onChange={(goal) => set({ goal })} />
      </Labelled>
      <Labelled label={t("workflow-goal-filter-fields-change")} hint={t("workflow-many-fields-any-of-checked")}>
        <WordChecks options={CHANGES} value={filter.change} fallback="closed" columns={3} disabled={disabled} onChange={(change) => set({ change })} />
      </Labelled>
    </div>
  );
}
