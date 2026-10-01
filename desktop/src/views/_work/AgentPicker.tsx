/**
 * The chip that names the agent a surface will post to, and lets the person
 * choose another — the act stays with the caller's own button. The editor's
 * selection toolbar wears it beside *Ask* and *Edit*; a rendered page's
 * annotation tray beside *Send* (ide/09). Its sibling `AgentMenu` is the
 * chip where picking **is** the act (*Fix with ▾*); this one only chooses.
 *
 * The choices are the ones every addressing surface on a checkout offers
 * (`agentChoices`): the chosen one leads with a check on it, the project's
 * `agents.default` is appended when it is not among them, and the name on
 * the chip is `chosenAgent`'s answer — never an agent the list would refuse.
 */

import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { settingOf } from "../../shell/settingsModel.mjs";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { FOCUS_RING, ICON, Menu, cn } from "../../ui";
import { GENERAL_AGENT, agentChoices, chosenAgent } from "../_workbench/editorAgentModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The agents a picker offers for a project and the one it names: `wanted`
 * when the person chose, else the project's `agents.default` — the General
 * Agent unless the project says otherwise.
 */
export function useAgentChoice(pid: string | null, wanted: string | null) {
  const ws = useWorkspace();
  const { resolved } = useResolvedSettings(pid);
  const setting = settingOf(resolved ?? [], "agents.default", GENERAL_AGENT);
  const defaultId = typeof setting === "string" && setting ? setting : GENERAL_AGENT;
  const lead = wanted ?? defaultId;
  const fallback = ws.agents.find((a) => a.id === defaultId) ?? null;
  const choices = agentChoices(ws.agents, lead, fallback ? { id: fallback.id, name: fallback.name } : null);
  return { choices, chosen: chosenAgent(choices, lead), defaultId };
}

export function AgentPicker({
  agentId,
  pid,
  onChoose,
  disabled = false,
}: {
  /** The agent the person chose, or `null` for the project's default; the chip names the one the choices allow. */
  agentId: string | null;
  /** The project, for its `agents.default`; null reads the workspace's. */
  pid: string | null;
  /** The person chose another: the caller keeps it and posts to it when it acts. */
  onChoose: (agent: string) => void;
  disabled?: boolean;
}) {
  const { choices, chosen, defaultId } = useAgentChoice(pid, agentId);
  const name = chosen?.name ?? agentId ?? defaultId;
  return (
    <Menu
      label={t("work-agent-picker-choose-agent")}
      items={choices.map((a) => ({
        label: a.harness ? `${a.name} · ${a.harness}` : a.name,
        icon: a.id === chosen?.id ? ICON.check : ICON.agent,
        disabled,
        onSelect: () => onChoose(a.id),
      }))}
      trigger={
        <span
          title={t("work-agent-picker-posting", { name })}
          className={cn(
            "anim inline-flex h-6 shrink-0 items-center gap-1 rounded-control border border-border px-1.5 text-2xs",
            disabled ? "cursor-not-allowed text-text-dim/60" : "text-text-dim hover:text-text",
            FOCUS_RING,
          )}
        >
          <ICON.agent size={11} aria-hidden />
          <span className="max-w-28 truncate">{name}</span>
          <ICON.collapsed size={10} aria-hidden />
        </span>
      }
    />
  );
}
