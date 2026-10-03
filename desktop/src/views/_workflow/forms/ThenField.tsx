/**
 * *Then* — where a step's flows go, picked without a pointer. The canvas
 * draws a flow with a drag from one handle to another; this is the same edit
 * from the keyboard: a plain step ticks the steps it flows into, a branching
 * step points each of its branches at a step. Every change is the canvas's
 * own (`workflowGraph.setThen`, `setBranchTarget`, both through `connect`),
 * so what the canvas refuses is refused here too, and said under the list.
 * A divert's path stays its boundary chip's.
 */

import { useState } from "react";
import type { Step } from "../../../types";
import { Checkbox, Labelled, Select } from "../../../ui";
import { setBranchTarget, setThen, thenChoices, type ConnectResult, type Definition } from "../workflowGraph.mjs";
import { t } from "../../../i18n/l10n.mjs";

export function ThenField({ wf, step, disabled, onChange }: { wf: Definition; step: Step; disabled?: boolean; onChange: (next: Definition) => void }) {
  const choices = thenChoices(wf, step.id);
  // Why the last pick was refused, until the next one lands.
  const [why, setWhy] = useState<string | null>(null);
  if (choices.targets.length === 0 && choices.branches.length === 0) return null;
  const apply = (r: ConnectResult<Definition>) => {
    if (r.ok) {
      setWhy(null);
      if (r.wf !== wf) onChange(r.wf);
    } else setWhy(r.reason);
  };
  const refused = why && (
    <p role="alert" className="mt-1 text-2xs text-danger">
      {why}
    </p>
  );

  if (choices.branching) {
    return (
      <Labelled label={t("workflow-then-field-then")} hint={t("workflow-then-field-branches-hint")}>
        <div className="flex flex-col gap-1.5">
          {choices.branches.map((b) => (
            <div key={b.branch} className="grid grid-cols-[minmax(5rem,auto)_minmax(0,1fr)] items-center gap-2">
              <code className="truncate font-mono text-2xs">{b.branch}</code>
              <Select value={b.to ?? ""} disabled={disabled} aria-label={t("workflow-then-field-branch-goes-to", { branch: b.branch })} onChange={(e) => apply(setBranchTarget(wf, step.id, b.branch, e.target.value || null))}>
                <option value="">{t("workflow-then-field-nowhere-yet")}</option>
                {/* A flow the validator refuses — into a start, a step that is gone — is shown as it is, to be moved off. */}
                {b.to && !choices.targets.some((x) => x.id === b.to) && <option value={b.to}>{b.to}</option>}
                {choices.targets.map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.label}
                  </option>
                ))}
              </Select>
            </div>
          ))}
        </div>
        {refused}
      </Labelled>
    );
  }

  return (
    <Labelled label={t("workflow-then-field-then")} hint={t("workflow-then-field-hint")}>
      <div role="group" aria-label={t("workflow-then-field-next-steps")} className="flex flex-col gap-1.5">
        {choices.targets.map((x) => (
          <Checkbox key={x.id} label={x.label} checked={x.on} disabled={disabled} onChange={(on) => apply(setThen(wf, step.id, x.id, on))} />
        ))}
      </div>
      {refused}
    </Labelled>
  );
}
