/**
 * An agent filter's fields — whose session, in which projects, on which
 * harnesses, doing what — shared by a start that begins when a harness
 * session moves and a wait that holds for one. An agent is one of the
 * staff, or a team for any member, or an input of kind assignee; a project
 * is named as a project filter names one; a harness by its id; each one or
 * several, or none for any. A worker in a goal's scratch folder is in no
 * project, so a project-narrowed filter never hears it.
 */

import type { InputDef } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Checkbox, Labelled } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { AGENT_CHANGES } from "../stepKinds.mjs";
import { fixedWords, inputNames, toggleInput, withFixed } from "./assigneeRefModel.mjs";
import type { AgentChange, AgentFilterEx } from "./eventFilters";
import { WordChecks, WordsInput } from "./ManyFields";
import { fromList } from "./manyModel.mjs";
import { ProjectRefsInput } from "./ProjectFilterFields";
import { asList } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

const CHANGES = AGENT_CHANGES.map((c) => ({ word: c.change as AgentChange, label: c.label }));

export function AgentFilterFields<F extends object>({ value, inputs, onChange, disabled }: { value: F; inputs: readonly InputDef[]; onChange: (next: F) => void; disabled?: boolean }) {
  const ws = useWorkspace();
  const filter = value as AgentFilterEx;
  const set = (patch: Partial<AgentFilterEx>) => onChange({ ...value, ...patch });
  const agents = asList(filter.agent);
  const assigneeInputs = inputs.filter((i) => i.kind === "assignee");
  const named = inputNames(agents);
  const harnesses = [...new Set(ws.agents.map((a) => a.harness))].sort().map((h) => ({ word: h, label: h }));
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-agent-filter-fields-agent")} hint={t("workflow-agent-filter-fields-agent-hint")}>
        <div className="flex flex-col gap-1.5">
          <AssigneePicker value={fixedWords(agents)} kinds={["agent", "team"]} disabled={disabled} onChange={(picked) => set({ agent: fromList(withFixed(agents, picked)) })} />
          {assigneeInputs.length > 0 && (
            <div className="flex flex-wrap gap-x-4 gap-y-1">
              {assigneeInputs.map((i) => (
                <Checkbox key={i.name} label={t("workflow-message-filter-fields-from-input", { name: i.name })} checked={named.includes(i.name)} disabled={disabled} onChange={() => set({ agent: fromList(toggleInput(agents, i.name)) })} />
              ))}
            </div>
          )}
        </div>
      </Labelled>
      <Labelled label={t("workflow-agent-filter-fields-project")} hint={t("workflow-agent-filter-fields-project-hint")}>
        <ProjectRefsInput value={filter.project} inputs={inputs} disabled={disabled} onChange={(project) => set({ project })} />
      </Labelled>
      <Labelled label={t("workflow-agent-filter-fields-harness")} hint={t("workflow-agent-filter-fields-harness-hint")}>
        <WordsInput mono value={filter.harness} disabled={disabled} suggestions={harnesses} /* for the machine */ placeholder="claude-code" aria-label={t("workflow-agent-filter-fields-harness")} onChange={(harness) => set({ harness })} />
      </Labelled>
      <Labelled label={t("workflow-agent-filter-fields-change")} hint={t("workflow-agent-filter-fields-change-hint")}>
        <WordChecks options={CHANGES} value={filter.change} fallback="done" columns={3} disabled={disabled} onChange={(change) => set({ change })} />
      </Labelled>
    </div>
  );
}
