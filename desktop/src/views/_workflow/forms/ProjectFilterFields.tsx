/**
 * A project filter's fields — which project, what change (a commit, a push
 * or a fetch, a pull request's change, a merge, a change of files), which
 * branch, which files — shared by a start that begins when a project
 * changes and a wait that holds for one. A project is named, never a path
 * on a disk: fixed, or read from an input of kind project; one not chosen
 * yet is `null`, a problem the validator names. The branch and the glob are
 * templates.
 */

import type { InputDef, ProjectChange, ProjectFilter } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Field, Select, TextInput } from "../../../ui";
import { PROJECT_CHANGES } from "../stepKinds.mjs";
import { ValueRefField } from "./AgentStepForm";
import { t } from "../../../i18n/l10n.mjs";

export function ProjectFilterFields<F extends ProjectFilter>({
  value,
  inputs,
  onChange,
  disabled,
}: {
  value: F;
  inputs: readonly InputDef[];
  onChange: (next: F) => void;
  disabled?: boolean;
}) {
  const ws = useWorkspace();
  const set = (patch: Partial<ProjectFilter>) => onChange({ ...value, ...patch });
  const change = value.change ?? "commit";
  const project = value.project ?? null;
  return (
    <div className="flex flex-col gap-3">
      <ValueRefField
        label={t("workflow-agent-step-form-project")}
        hint={t("workflow-project-filter-fields-project-hint")}
        value={project && typeof project === "object" ? { input: project.input } : project}
        inputs={[...inputs]}
        kind="project"
        disabled={disabled}
        onChange={(v) => set({ project: v })}
      >
        {(fixed, setFixed) => (
          <Select value={fixed} disabled={disabled} onChange={(e) => setFixed(e.target.value)}>
            <option value="">{t("workflow-inputs-form-pick-project")}</option>
            {ws.projects.map((p) => (
              <option key={p.project.id} value={p.project.id}>
                {p.project.name}
              </option>
            ))}
          </Select>
        )}
      </ValueRefField>
      <div className="grid gap-3 @xs:grid-cols-2">
        <Field label={t("workflow-project-filter-fields-change")}>
          <Select value={change} disabled={disabled} onChange={(e) => set({ change: e.target.value as ProjectChange })}>
            {PROJECT_CHANGES.map((c) => (
              <option key={c.change} value={c.change}>
                {c.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("workflow-project-filter-fields-branch")} hint={t("workflow-project-filter-fields-branch-hint")}>
          <TextInput className="font-mono" value={value.branch ?? ""} disabled={disabled} onChange={(e) => set({ branch: e.target.value.trim() || null })} />
        </Field>
      </div>
      {change === "files" && (
        <Field label={t("workflow-project-filter-fields-files")} hint={t("workflow-project-filter-fields-files-hint")}>
          <TextInput className="font-mono" value={value.glob ?? ""} /* for the machine */ placeholder="*.csv" disabled={disabled} onChange={(e) => set({ glob: e.target.value.trim() || null })} />
        </Field>
      )}
      {(change === "pull_request" || change === "merge") && <p className="text-2xs text-text-dim">{t("workflow-project-filter-fields-pull-requests-note")}</p>}
    </div>
  );
}
