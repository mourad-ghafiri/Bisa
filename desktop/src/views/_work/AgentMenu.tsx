/**
 * One compact door that hands something to an agent of the person's choice
 * — *Fix with ▾* on a comment's row, on the Comments header, on a failed
 * check's row (ide/08): a chip that opens the reachable agents
 * (`agentChoices`, the filter every addressing surface on a checkout
 * applies), the remembered one first with a check on it, the project's
 * `agents.default` appended when it is not among them. Picking an agent
 * **is** the act: the caller asks and remembers in one move, so no row ever
 * names an agent before the person chose. The same chip
 * `SelectionAgentBar` uses for the editor's selection.
 *
 * The `hint` is the menu's accessible name and the trigger's native `title`
 * — never a floating `Tooltip`: a menu opens where a tooltip would sit, and
 * a tooltip over the list is a list nobody can pick from.
 */

import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { settingOf } from "../../shell/settingsModel.mjs";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { FOCUS_RING, ICON, Menu, cn } from "../../ui";
import { GENERAL_AGENT, agentChoices } from "../_workbench/editorAgentModel.mjs";

export function AgentMenu({
  label,
  remembered,
  pid,
  hint,
  disabled = false,
  onPick,
}: {
  /** The words on the chip — *Fix with*. */
  label: string;
  /** The agent the list leads with: the last one used for this kind of ask. */
  remembered: string;
  /** The project, for its `agents.default`; null reads the workspace's. */
  pid: string | null;
  /** What picking does, in one sentence: the menu's name, the trigger's title. */
  hint: string;
  disabled?: boolean;
  /** The person chose: the caller asks this agent and remembers it. */
  onPick: (agent: string) => void;
}) {
  const ws = useWorkspace();
  const { resolved } = useResolvedSettings(pid);
  const defaultId = settingOf(resolved ?? [], "agents.default", GENERAL_AGENT);
  const fallback = ws.agents.find((a) => a.id === defaultId) ?? null;
  const choices = agentChoices(ws.agents, remembered, fallback ? { id: fallback.id, name: fallback.name } : null);
  return (
    <Menu
      label={hint}
      items={choices.map((a) => ({
        label: a.harness ? `${a.name} · ${a.harness}` : a.name,
        icon: a.id === remembered ? ICON.check : ICON.agent,
        disabled,
        onSelect: () => onPick(a.id),
      }))}
      trigger={
        <span
          title={hint}
          className={cn(
            "anim inline-flex h-6 shrink-0 items-center gap-1 rounded-control border border-border px-1.5 text-2xs",
            disabled ? "cursor-not-allowed text-text-dim/60" : "text-text-dim hover:text-text",
            FOCUS_RING,
          )}
        >
          <ICON.agent size={11} aria-hidden />
          <span>{label}</span>
          <ICON.collapsed size={10} aria-hidden />
        </span>
      }
    />
  );
}
