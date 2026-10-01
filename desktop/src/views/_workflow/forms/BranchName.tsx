/**
 * One branch name of a gateway — a `decide`'s rule, a `switch`'s case, a
 * `judge`'s option, or the step's `otherwise` — committed when the field is
 * left rather than per keystroke: a half-typed name would otherwise orphan
 * the flow three times on the way to its name. The rename is
 * `workflowGraph.relabelBranch`'s — it carries the rule, the case or the
 * option, the `otherwise` and every flow along; a refused name reverts and
 * says why under the field. One field for the three forms.
 */

import { useEffect, useState } from "react";
import type { Step } from "../../../types";
import { TextInput } from "../../../ui";
import { relabelBranch } from "../workflowGraph.mjs";
import { t } from "../../../i18n/l10n.mjs";

export function BranchName({
  step,
  branch,
  disabled,
  className,
  onChange,
}: {
  step: Extract<Step, { kind: "decide" | "switch" | "judge" }>;
  branch: string;
  disabled?: boolean;
  className?: string;
  onChange: (next: Step) => void;
}) {
  const [draft, setDraft] = useState(branch);
  const [why, setWhy] = useState<string | null>(null);
  useEffect(() => setDraft(branch), [branch]);
  const commit = () => {
    const r = relabelBranch(step, branch, draft);
    if (r.ok) {
      setWhy(null);
      if (r.step !== step) onChange(r.step);
    } else {
      setWhy(r.reason);
      setDraft(branch);
    }
  };
  return (
    <span className="flex flex-col gap-0.5">
      <TextInput
        className={className}
        value={draft}
        aria-label={t("workflow-branch-name-branch")}
        disabled={disabled}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
      />
      {why && <span className="text-2xs text-danger">{why}</span>}
    </span>
  );
}
