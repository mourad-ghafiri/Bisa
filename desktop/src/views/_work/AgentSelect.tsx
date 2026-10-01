/**
 * One agent, picked from a plain `Select` — no search, no grouping, no
 * teams: the agents a checkout's conversation can reach (`agentChoices`, the
 * same filter every addressing surface on a checkout applies), the one last
 * used first. An agent the workspace no longer has is still shown, named by
 * its id, so a remembered choice never turns into a silent swap.
 */

import { useWorkspace } from "../../shell/useWorkspaceData";
import { Select } from "../../ui";
import { agentChoices } from "../_workbench/editorAgentModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function AgentSelect({
  value,
  onChange,
  disabled,
  label = t("work-review-step-agent"),
  className = "min-w-0 flex-1",
}: {
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
  label?: string;
  /** The control's width: the row's whole width by default, narrower on a header. */
  className?: string;
}) {
  const ws = useWorkspace();
  const choices = agentChoices(ws.agents, value, null);
  const known = choices.some((a) => a.id === value);
  return (
    <Select aria-label={label} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)} className={className}>
      {!known && value && <option value={value}>{t("work-agent-select-not-workspace", { value })}</option>}
      {choices.map((a) => (
        <option key={a.id} value={a.id}>
          {a.name}
          {a.harness ? ` · ${a.harness}` : ""}
        </option>
      ))}
    </Select>
  );
}
