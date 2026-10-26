/**
 * A workflow filter's fields — which definitions' moves in the library, and
 * which moves — shared by a start that begins when a workflow moves and a
 * wait that holds for one. A workflow is named by its id, one or several,
 * or none for any; never checked against the library, so a start on a
 * workflow's `deleted` fires when it goes.
 */

import { Labelled } from "../../../ui";
import { WORKFLOW_CHANGES } from "../stepKinds.mjs";
import type { WorkflowChange, WorkflowFilterEx } from "./eventFilters";
import { WordChecks } from "./ManyFields";
import { WorkflowRefsInput } from "./RunFilterFields";
import { t } from "../../../i18n/l10n.mjs";

const CHANGES = WORKFLOW_CHANGES.map((c) => ({ word: c.change as WorkflowChange, label: c.label }));

export function WorkflowFilterFields<F extends object>({ value, onChange, disabled }: { value: F; onChange: (next: F) => void; disabled?: boolean }) {
  const filter = value as WorkflowFilterEx;
  const set = (patch: Partial<WorkflowFilterEx>) => onChange({ ...value, ...patch });
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-workflow-filter-fields-workflows")} hint={t("workflow-workflow-filter-fields-workflows-hint")}>
        <WorkflowRefsInput value={filter.workflow} disabled={disabled} addLabel={t("workflow-run-filter-fields-add-workflow")} onChange={(workflow) => set({ workflow })} />
      </Labelled>
      <Labelled label={t("workflow-workflow-filter-fields-change")} hint={t("workflow-many-fields-any-of-checked")}>
        <WordChecks options={CHANGES} value={filter.change} fallback="saved" columns={3} disabled={disabled} onChange={(change) => set({ change })} />
      </Labelled>
    </div>
  );
}
